//! Provider-free fault injection for graded siblings plus one reserved slot.
#![cfg(unix)]
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Project(PathBuf);
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn asb(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asb"));
    command
        .current_dir(root)
        .args(args)
        .env_clear()
        .env("HOME", root)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command.output().unwrap()
}
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn rows(output: &Output, success: bool) -> Vec<Value> {
    assert_eq!(
        output.status.success(),
        success,
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn partial_sweep_preserves_graded_siblings_and_runs_only_a_positive_reserved_slot() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let project =
        Project(std::env::temp_dir().join(format!("asb-partial-{}-{stamp}", std::process::id())));
    let root = &project.0;
    std::fs::create_dir_all(root).unwrap();
    let initialized = asb(root, &["init"]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let binary = root.join(".asb-tools/pi/bin/pi");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary,r##"#!/usr/bin/env node
const fs=require('fs');
const slot=Number(process.cwd().match(/workspace-concurrency-(\d+)$/)[1]);
fs.appendFileSync(process.env.HOME+'/launches',slot+'\n');
const config=JSON.parse(fs.readFileSync(process.env.PI_CODING_AGENT_DIR+'/models.json'));
const base=config.providers.asb.baseUrl;
if(!/^http:\/\/127\.0\.0\.1:\d+\/v1$/.test(base)) throw new Error('not loopback');
fetch(base+'/responses',{method:'POST',headers:{'content-type':'application/json'},
  body:JSON.stringify({model:'test-model',input:'offline fixture',tools:[{type:'function'}]})})
  .then(async response=>{if(!response.ok) throw new Error(await response.text());
    if(slot!==1) fs.writeFileSync('src/parser.py',"def parse_path(value: str) -> list[str]:\n    return [segment for segment in value.split('/') if segment]\n");
    console.log('{"status":"ok"}');})
  .catch(error=>{console.error(error.message);process.exitCode=1;});
"##).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.join("benchmark.toml"),"agent='pi'\nworkload='bug-fix'\nprovider='cli2key'\nmodel='test-model'\nconcurrency=3\n[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n").unwrap();
    std::fs::write(root.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let tapes = root.join("tapes/000-benchmark/cassettes");
    std::fs::create_dir_all(&tapes).unwrap();
    let interaction = json!({"request":{"method":"POST","url":"/v1/responses","body":
        "{\"model\":\"test-model\",\"input\":\"offline fixture\",\"tools\":[{\"type\":\"function\"}]}"},
        "response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}});
    std::fs::write(
        tapes.join("benchmark-pi-bug-fix-test-model.jsonl"),
        format!("{interaction}\n"),
    )
    .unwrap();
    let args = [
        "sweep",
        "sweep.toml",
        "--replay",
        "--cassette-root",
        "tapes",
        "--json",
    ];
    // Slot 1 deliberately exits zero without repairing its task. Its false
    // official local grade must remain completed, not be retried until green.
    let original = rows(&asb(root, &args), false);
    assert_eq!(original.len(), 3);
    let results = root.join("sweep-results");
    let directory = results.join("000-benchmark");
    let checkpoint = results.join(".checkpoint");
    let first_receipt = std::fs::read(checkpoint.join("replicas/000000/001.json")).unwrap();
    let first_log = std::fs::read(directory.join("run-concurrency-1.log")).unwrap();
    let initial = read(checkpoint.join("inputs.json"))["initial"].clone();
    let replica_path = checkpoint.join("replicas/000000/002.json");
    let replica = read(&replica_path);
    let outcome_path = PathBuf::from(replica["outcome"].as_str().unwrap());
    let outcome = read(&outcome_path);
    let previous_attempt = outcome["summary"]["task"]["attempt_id"].clone();
    let started_path = checkpoint.join("started/000000/002.json");
    let started = read(&started_path);
    let archive = root.join("simulated-before-slot-2-start");
    std::fs::create_dir(&archive).unwrap();
    // This is controlled state-machine fault injection after all fixture
    // processes stopped, not a claim to recover a live crashed container.
    for (index, output) in outcome["outputs"].as_array().unwrap().iter().enumerate() {
        let output = Path::new(output.as_str().unwrap());
        assert!(output.starts_with(&directory) && output != directory);
        std::fs::rename(output, archive.join(format!("output-{index}"))).unwrap();
    }
    std::fs::rename(&outcome_path, archive.join("outcome.json")).unwrap();
    std::fs::rename(&replica_path, archive.join("replica.json")).unwrap();
    std::fs::rename(
        checkpoint.join("completed/000000.json"),
        archive.join("group.json"),
    )
    .unwrap();
    let continuation = [
        "sweep",
        "sweep.toml",
        "--replay",
        "--cassette-root",
        "tapes",
        "--resume",
        "--json",
    ];
    let rejected = asb(root, &continuation);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("already-started slot"));
    assert_eq!(
        std::fs::read_to_string(root.join("launches"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    std::fs::rename(&started_path, archive.join("started.json")).unwrap();
    let reservation = json!({"schema_version":1,"scope":"reserved-replica-slot","configuration":0,
        "replica":started["replica"],"input_digest":started["input_digest"],
        "entry":started["entry"],"reservation_id":started["reservation_id"]});
    let pending = checkpoint.join("pending/000000/002.json");
    std::fs::write(&pending, serde_json::to_vec(&reservation).unwrap()).unwrap();
    std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o600)).unwrap();
    let resumed = rows(&asb(root, &continuation), false);
    assert_eq!(resumed.len(), 3);
    assert_eq!(
        resumed.iter().filter(|row| row["reused"] == true).count(),
        2
    );
    assert_eq!(
        resumed
            .iter()
            .filter(|row| row["workload_success"] == false)
            .count(),
        1
    );
    assert!(resumed.iter().all(|row| row["process_success"] == true));
    let launches = std::fs::read_to_string(root.join("launches")).unwrap();
    for (slot, count) in [("1", 1), ("2", 2), ("3", 1)] {
        assert_eq!(launches.lines().filter(|line| *line == slot).count(), count);
    }
    let fresh = resumed.iter().find(|row| row["reused"] != true).unwrap();
    assert_ne!(fresh["task"]["attempt_id"], previous_attempt);
    assert_eq!(read(&replica_path)["replica"], json!({"index":2,"total":3}));
    assert_eq!(
        std::fs::read(checkpoint.join("replicas/000000/001.json")).unwrap(),
        first_receipt
    );
    assert_eq!(
        std::fs::read(directory.join("run-concurrency-1.log")).unwrap(),
        first_log
    );
    for row in resumed.iter().filter(|row| row["reused"] == true) {
        assert!(original.iter().any(|old| old["task"] == row["task"]));
    }
    assert!(!pending.exists());
    let seal = read(checkpoint.join("inputs.json"));
    assert_eq!(seal["status"], "stable");
    assert_eq!(seal["initial"], initial);
    assert_eq!(
        read(root.join("results/sweep-comparison/comparison.json"))["evaluation_coverage"]["evaluated_attempts"],
        3
    );
    assert!(
        archive.join("group.json").exists(),
        "all replaced fixture evidence remains preserved"
    );
}
