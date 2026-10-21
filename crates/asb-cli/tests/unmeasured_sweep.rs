//! Real CLI, replay-only loopback provider, no runtime measurement collection.
#![cfg(unix)]
use serde_json::{Value, json};
mod support;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use support::IsolatedEnvironment;

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
        .isolated_env()
        .env("HOME", root)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default());
    command.output().unwrap()
}

fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn unmeasured_sweep_compares_actual_grades_and_resumes_without_repeating_clients() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let project = Project(
        std::env::temp_dir().join(format!("asb-unmeasured-{}-{stamp}", std::process::id())),
    );
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
    if(slot===2) fs.writeFileSync('src/parser.py',"def parse_path(value: str) -> list[str]:\n    return [segment for segment in value.split('/') if segment]\n");
    console.log('{"status":"ok"}');
    // A process failure must not erase a separately graded correct submission.
    if(slot===2) process.exitCode=7;
  }).catch(error=>{console.error(error.message);process.exitCode=1;});
"##).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (index, name, settings) in [
        (0, "default", ""),
        (
            1,
            "disabled",
            "[measurements]\nenabled=false\ndirectory='measurements'\ncollectors=[]\n",
        ),
    ] {
        std::fs::write(root.join(format!("{name}.toml")),format!("agent='pi'\nprovider='cli2key'\nmodel='test-model'\nworkload='bug-fix'\nconcurrency=2\n{settings}")).unwrap();
        let directory = root.join(format!("tapes/{index:03}-{name}/cassettes"));
        std::fs::create_dir_all(&directory).unwrap();
        let interaction = json!({"request":{"method":"POST","url":"/v1/responses",
            "body":"{\"model\":\"test-model\",\"input\":\"offline fixture\",\"tools\":[{\"type\":\"function\"}]}"},
            "response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}});
        std::fs::write(
            directory.join("benchmark-pi-bug-fix-test-model.jsonl"),
            format!("{interaction}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join("sweep.toml"),
        "configs=['default.toml','disabled.toml']\n",
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
    let output = asb(root, &args);
    assert!(
        !output.status.success(),
        "one unresolved and one failed process per configuration"
    );
    let rows: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 4, "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        rows.iter()
            .filter(|row| row["workload_success"] == true)
            .count(),
        2
    );
    assert!(
        rows.iter()
            .filter(|row| row["workload_success"] == true)
            .all(|row| row["process_success"] == false)
    );
    let report_path = root.join("results/sweep-comparison/comparison.json");
    let report = read(&report_path);
    assert_eq!(report["evaluation_coverage"]["evaluated_attempts"], 4);
    let group = report["resolution_groups"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(group["success_rate"], 0.5);
    let compared = report["benchmarks"].as_array().unwrap();
    assert!(
        compared
            .iter()
            .all(|row| row["result_source"] == "task-outcome")
    );
    assert_eq!(
        compared
            .iter()
            .filter(|row| row["metrics"]["success_rate"] == 1.0)
            .count(),
        2
    );
    for name in ["000-default", "001-disabled"] {
        let directory = root.join("sweep-results").join(name);
        assert!(!directory.join("measurements-concurrency-1").exists());
        assert!(!directory.join("measurements-concurrency-2").exists());
        assert_eq!(
            std::fs::read_dir(directory.join(".asb-completions"))
                .unwrap()
                .count(),
            2
        );
    }
    let launches = std::fs::read(root.join("launches")).unwrap();
    let compared_again = asb(root, &["compare", "sweep.toml"]);
    assert!(
        compared_again.status.success(),
        "{}",
        String::from_utf8_lossy(&compared_again.stderr)
    );
    assert_eq!(read(&report_path), report);
    let mut resume = args.to_vec();
    resume.push("--resume");
    let resumed = asb(root, &resume);
    assert!(!resumed.status.success());
    let reused: Vec<Value> = String::from_utf8_lossy(&resumed.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        reused.len(),
        4,
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(reused.iter().all(|row| row["reused"] == true));
    assert_eq!(
        reused
            .iter()
            .filter(|row| row["workload_success"] == true)
            .count(),
        2
    );
    assert_eq!(std::fs::read(root.join("launches")).unwrap(), launches);
    // Exercise actual continuation admission against invalid completed groups.
    // A valid earlier group cannot cause scheduling or receipt publication
    // before a later malformed member has been checked.
    let outcomes = root.join("sweep-results/001-disabled/.asb-completions");
    let outcome = std::fs::read_dir(&outcomes)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let original = std::fs::read(&outcome).unwrap();
    let raw_outcomes: Vec<_> = ["000-default", "001-disabled"]
        .into_iter()
        .flat_map(|group| {
            std::fs::read_dir(
                root.join("sweep-results")
                    .join(group)
                    .join(".asb-completions"),
            )
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
        })
        .collect();
    let checkpoint = root.join("sweep-results/.checkpoint");
    let receipt_files: Vec<_> = [
        "completed/000000.json",
        "completed/000001.json",
        "replicas/000000/001.json",
        "replicas/000000/002.json",
        "replicas/000001/001.json",
        "replicas/000001/002.json",
    ]
    .into_iter()
    .map(|path| {
        let path = checkpoint.join(path);
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect();
    let slot = read(&outcome)["replica"]["index"].as_u64().unwrap();
    let unsealed_paths = [
        checkpoint.join("completed/000001.json"),
        checkpoint.join(format!("replicas/000001/{slot:03}.json")),
    ];
    for (pointer, replacement, diagnostic) in [
        (
            "/summary/workload_success",
            Value::Null,
            "structured outcome resolution differs from its summary",
        ),
        (
            "/summary/process_success",
            json!("unknown"),
            "outcome grade, evaluator or baseline is incomplete",
        ),
        (
            "/workload_cleanup_confirmed",
            json!(false),
            "outcome workload or proxy cleanup is unconfirmed",
        ),
        (
            "/replica/total",
            json!(3),
            "outcome replica does not match the selected population",
        ),
        (
            "/outputs/0",
            json!("relative"),
            "declared execution output path is relative",
        ),
    ] {
        let mut changed: Value = serde_json::from_slice(&original).unwrap();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let changed = serde_json::to_vec(&changed).unwrap();
        std::fs::write(&outcome, &changed).unwrap();
        for reconcile in [false, true] {
            let mut invalid_args = resume.clone();
            if reconcile {
                invalid_args.push("--reconcile-completed");
            }
            let rejected = asb(root, &invalid_args);
            assert!(
                !rejected.status.success(),
                "{pointer}, reconciliation={reconcile}"
            );
            assert!(
                String::from_utf8_lossy(&rejected.stderr)
                    .contains("completed task cannot be reused"),
                "{}",
                String::from_utf8_lossy(&rejected.stderr)
            );
            assert_eq!(std::fs::read(root.join("launches")).unwrap(), launches);
            assert_eq!(std::fs::read(&outcome).unwrap(), changed);
            for (path, bytes) in &raw_outcomes {
                assert_eq!(
                    &std::fs::read(path).unwrap(),
                    if path == &outcome { &changed } else { bytes }
                );
            }
            for (path, bytes) in &receipt_files {
                assert_eq!(&std::fs::read(path).unwrap(), bytes);
            }
        }
        // Native tasks have no allocated official environment to reconcile.
        // Retain the already-valid earlier group and sibling seals, removing
        // only this late member's replica seal and its enclosing group seal.
        // The invalid raw outcome must reject before allocation inspection or
        // receipt publication, rather than merely failing an existing hash.
        // Unknown process observations and malformed populations are already
        // rejected by the earlier report-only grade-closure precheck.
        if matches!(pointer, "/summary/process_success" | "/replica/total") {
            continue;
        }
        for path in &unsealed_paths {
            std::fs::remove_file(path).unwrap();
        }
        for reconcile in [false, true] {
            let mut invalid_args = resume.clone();
            if reconcile {
                invalid_args.push("--reconcile-completed");
            }
            let rejected = asb(root, &invalid_args);
            assert!(
                !rejected.status.success(),
                "unsealed {pointer}, reconciliation={reconcile}"
            );
            assert!(
                String::from_utf8_lossy(&rejected.stderr).contains(diagnostic),
                "unsealed {pointer}: {}",
                String::from_utf8_lossy(&rejected.stderr)
            );
            assert_eq!(std::fs::read(root.join("launches")).unwrap(), launches);
            assert_eq!(std::fs::read(&outcome).unwrap(), changed);
            for (path, bytes) in &raw_outcomes {
                assert_eq!(
                    &std::fs::read(path).unwrap(),
                    if path == &outcome { &changed } else { bytes }
                );
            }
            for (path, bytes) in &receipt_files {
                if unsealed_paths.contains(path) {
                    assert!(
                        !path.exists(),
                        "no newly published receipt: {}",
                        path.display()
                    );
                } else {
                    assert_eq!(&std::fs::read(path).unwrap(), bytes);
                }
            }
        }
        for (path, bytes) in &receipt_files {
            if unsealed_paths.contains(path) {
                std::fs::write(path, bytes).unwrap();
            }
        }
    }
    std::fs::write(&outcome, original).unwrap();
    let html = std::fs::read_to_string(root.join("results/sweep-comparison/report.html")).unwrap();
    assert!(html.contains("optional measurements unavailable"));
    assert!(html.contains("metric-success_rate.svg"));
}
