//! Distinct tasks plus replicas, real loopback replay, no live clients/providers.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Project(PathBuf);
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn asb(root: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asb"));
    command
        .args(arguments)
        .current_dir(root)
        .env_clear()
        .env("HOME", root)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    command.output().unwrap()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn completions(root: &Path, name: &str) -> Vec<serde_json::Value> {
    std::fs::read_dir(
        root.join("sweep-results")
            .join(name)
            .join(".asb-completions"),
    )
    .unwrap()
    .map(|entry| {
        let path = entry.unwrap().path();
        assert_eq!(path.extension().unwrap(), "json");
        read_json(&path)
    })
    .collect()
}

#[test]
fn distinct_tasks_and_replicas_overlap_with_isolated_replay_outputs_and_failures() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let project =
        Project(std::env::temp_dir().join(format!("asb-parallel-{}-{stamp}", std::process::id())));
    let root = &project.0;
    std::fs::create_dir_all(root).unwrap();
    let initialized = asb(root, &["init"]);
    assert!(
        initialized.status.success(),
        "{}",
        String::from_utf8_lossy(&initialized.stderr)
    );
    let tool = root.join(".asb-tools/pi/bin");
    std::fs::create_dir_all(&tool).unwrap();
    let agent = tool.join("pi");
    std::fs::write(&agent, r##"#!/usr/bin/env node
const fs=require('fs'); const home=process.env.HOME;
const first=process.cwd().includes('000-first');
const replica=process.cwd().match(/concurrency-(\d+)/)?.[1];
const name=first?'first-'+replica:'second';
fs.writeFileSync(home+'/started-'+name,'started');
const expected=['first-1','first-2','second'];
(async()=>{
  const streaming=fs.existsSync(home+'/stream-test');
  if(streaming) console.log('fixture streaming ready');
  const deadline=Date.now()+5000;
  while(streaming ? !fs.existsSync(home+'/stream-ack') : !expected.every(x=>fs.existsSync(home+'/started-'+x))) {
    if(Date.now()>deadline) throw new Error('distinct tasks and replicas did not overlap');
    await new Promise(resolve=>setTimeout(resolve,10));
  }
  if(first && fs.existsSync(home+'/fail-first')) process.exit(7);
  const conf=JSON.parse(fs.readFileSync(process.env.PI_CODING_AGENT_DIR+'/models.json'));
  const url=conf.providers.asb.baseUrl;
  if(!/^http:\/\/127\.0\.0\.1:\d+\/v1$/.test(url)) throw new Error('not loopback');
  const body={model:'fixture-model',input:'offline request',tools:[{type:'function'}]};
  const response=await fetch(url+'/responses',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
  if(!response.ok) throw new Error('replay mismatch');
  fs.writeFileSync(first?'src/parser.py':'src/formatter.py', first?
    "def parse_path(value: str) -> list[str]:\n    return [segment for segment in value.split('/') if segment]\n":
    'def format_items(items: list[str], reverse: bool = False) -> str:\n    ordered = list(reversed(items)) if reverse else items\n    return ", ".join(ordered)\n');
  console.log(JSON.stringify({type:'result',result:'fixture repaired'}));
})().catch(error=>{console.error(error.message);process.exitCode=1;});
"##).unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (name, workload, replicas, index) in [
        ("first", "bug-fix", 2, 0),
        ("second", "feature-addition", 1, 1),
    ] {
        std::fs::write(root.join(format!("{name}.toml")), format!("agent='pi'\nprovider='cli2key'\nmodel='fixture-model'\nworkload='{workload}'\nconcurrency={replicas}\n[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n")).unwrap();
        let tapes = root.join(format!("tapes/{index:03}-{name}/cassettes"));
        std::fs::create_dir_all(&tapes).unwrap();
        let body =
            r#"{"model":"fixture-model","input":"offline request","tools":[{"type":"function"}]}"#;
        let exchange = serde_json::json!({"request":{"method":"POST","url":"/v1/responses","body":body},"response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}});
        std::fs::write(
            tapes.join(format!("benchmark-pi-{workload}-fixture-model.jsonl")),
            format!("{exchange}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("sweep.toml"),
        "configs=['first.toml','second.toml']\n",
    )
    .unwrap();
    let arguments = [
        "sweep",
        "sweep.toml",
        "--jobs",
        "2",
        "--max-active-attempts",
        "3",
        "--replay",
        "--cassette-root",
        "tapes",
        "--json",
    ];
    let output = asb(root, &arguments);
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows.len(),
        3,
        "parallel output must remain complete JSONL records"
    );
    for row in rows {
        assert_eq!(row["success_rate"], 1.0, "{row}");
        assert_eq!(row["agent"], "pi");
        assert_eq!(row["provider"], "cli2key");
        assert_eq!(row["model"], "fixture-model");
        assert!(row["workload"].as_str().is_some());
    }
    let comparison = root.join("results/sweep-comparison/comparison.json");
    for (name, count) in [("000-first", 2), ("001-second", 1)] {
        let records = completions(root, name);
        assert_eq!(records.len(), count);
        let mut attempts = std::collections::BTreeSet::new();
        for record in records {
            assert_eq!(record["summary"]["workload_success"], true);
            assert_eq!(record["workload_cleanup_confirmed"], true);
            attempts.insert(
                record["summary"]["task"]["attempt_id"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        assert_eq!(attempts.len(), count);
    }
    let seal = read_json(&root.join("sweep-results/.checkpoint/inputs.json"));
    assert_eq!(seal["scope"], "input-stability-only-not-a-resume-receipt");
    assert_eq!(seal["status"], "stable");
    let report = read_json(&comparison);
    assert_eq!(report["benchmarks"].as_array().unwrap().len(), 3);
    assert_eq!(report["evaluation_coverage"]["evaluated_attempts"], 3);
    assert_eq!(report["selection_plan"]["scheduling"]["jobs"], 2);
    assert_eq!(
        report["selection_plan"]["scheduling"]["max_active_attempts"],
        3
    );
    let html = std::fs::read_to_string(root.join("results/sweep-comparison/report.html")).unwrap();
    assert!(html.contains("2 distinct configurations; 3 active attempt slots"));
    for name in ["000-first", "001-second"] {
        assert!(
            !root
                .join("sweep-results")
                .join(name)
                .join(".terminal-output.tmp")
                .exists()
        );
    }

    // A one-job sweep must not turn --show-output into delayed output. The
    // client cannot complete until its streamed line is observed and acked.
    std::fs::write(root.join("stream-test"), "fixture streaming gate").unwrap();
    std::fs::write(root.join("stream.toml"), "configs=['second.toml']\n").unwrap();
    let tapes = root.join("stream-tapes/000-second/cassettes");
    std::fs::create_dir_all(&tapes).unwrap();
    std::fs::copy(
        root.join("tapes/001-second/cassettes/benchmark-pi-feature-addition-fixture-model.jsonl"),
        tapes.join("benchmark-pi-feature-addition-fixture-model.jsonl"),
    )
    .unwrap();
    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_asb"))
            .args([
                "sweep",
                "stream.toml",
                "--replay",
                "--cassette-root",
                "stream-tapes",
                "--show-output",
            ])
            .current_dir(root)
            .env_clear()
            .env("HOME", root)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stream = process.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stream).lines().take(256) {
            if send.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut observed = false;
    while let Ok(line) =
        receive.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        if line.contains("fixture streaming ready") {
            observed = true;
            break;
        }
    }
    std::fs::write(root.join("stream-ack"), "observed").unwrap();
    assert!(
        observed,
        "one-job --show-output must stream before the client completes"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "streaming fixture did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(status.success());
    reader.join().unwrap();
    let saved_plan = std::fs::read(root.join("sweep-results/plan.json")).unwrap();
    let saved_report = std::fs::read(&comparison).unwrap();
    let invalid = asb(
        root,
        &[
            "sweep",
            "sweep.toml",
            "--jobs",
            "2",
            "--max-active-attempts",
            "1",
            "--replay",
        ],
    );
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("increase --max-active-attempts"));
    assert_eq!(
        std::fs::read(root.join("sweep-results/plan.json")).unwrap(),
        saved_plan
    );
    assert_eq!(std::fs::read(&comparison).unwrap(), saved_report);

    for name in ["first-1", "first-2", "second"] {
        std::fs::remove_file(root.join(format!("started-{name}"))).unwrap();
    }
    std::fs::write(root.join("fail-first"), "fixture failure").unwrap();
    let failed = asb(root, &arguments);
    assert!(!failed.status.success());
    let rows: Vec<serde_json::Value> = String::from_utf8(failed.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 3);
    for record in completions(root, "000-first") {
        assert_eq!(record["summary"]["workload_success"], false);
        assert_eq!(record["workload_cleanup_confirmed"], true);
    }
    let report = read_json(&comparison);
    assert_eq!(report["benchmarks"].as_array().unwrap().len(), 3);
    assert_eq!(report["evaluation_coverage"]["evaluated_attempts"], 3);
    let benchmarks = report["benchmarks"].as_array().unwrap();
    assert_eq!(
        benchmarks
            .iter()
            .filter(|row| row["metrics"]["success_rate"] == 1.0)
            .count(),
        1,
        "the independent second task must succeed while both first-task replicas fail: {report}"
    );
    for name in ["000-first", "001-second"] {
        assert!(
            !root
                .join("sweep-results")
                .join(name)
                .join(".terminal-output.tmp")
                .exists()
        );
    }
}
