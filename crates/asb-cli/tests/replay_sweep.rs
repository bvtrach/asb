//! Offline end-to-end sweep replay using a fake agent and a real loopback tape.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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
    command.output().expect("ASB starts")
}

#[test]
fn sweep_replays_one_recording_eight_times_without_a_live_bridge() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("asb-sweep-replay-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let init = asb(&root, &["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let agent_dir = root.join(".asb-tools/pi/bin");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let agent = agent_dir.join("pi");
    std::fs::write(&agent, r##"#!/usr/bin/env node
const fs = require('fs');
const conf = JSON.parse(fs.readFileSync(process.env.PI_CODING_AGENT_DIR + '/models.json'));
const url = conf.providers.asb.baseUrl;
if (conf.providers.asb.api !== 'openai-responses') process.exit(4);
if (!/^http:\/\/127\.0\.0\.1:\d+\/v1$/.test(url || '')) process.exit(3);
const body = {model: 'test-model', input: 'example prompt', tools: [{type: 'function'}]};
fetch(url + '/responses', {method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify(body)})
  .then(async r => { if (!r.ok) throw new Error(await r.text()); console.log('{"status":"ok"}'); })
  .catch(e => { console.error(e); process.exitCode = 1; });
"##).unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        root.join("benchmark.toml"),
        "agent='pi'\nworkload='feature-addition'\nprovider='cli2key'\nmodel='test-model'\nconcurrency=8\n[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n",
    )
    .unwrap();
    std::fs::write(root.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let cassette_dir = root.join("tapes/000-benchmark/cassettes");
    std::fs::create_dir_all(&cassette_dir).unwrap();
    let cassette = cassette_dir.join("benchmark-pi-feature-addition-test-model.jsonl");
    let body = r#"{"model":"test-model","input":"example prompt","tools":[{"type":"function"}]}"#;
    let interaction = serde_json::json!({"request":{"method":"POST","url":"/v1/responses","body":body},"response":{"status":200,"headers":{"content-type":"application/json"},"body":"{}"}});
    std::fs::write(&cassette, format!("{interaction}\n")).unwrap();
    let result = asb(
        &root,
        &[
            "sweep",
            "sweep.toml",
            "--replay",
            "--cassette-root",
            "tapes",
        ],
    );
    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    for index in 1..=8 {
        let log = root.join(format!(
            "sweep-results/000-benchmark/run-concurrency-{index}.log"
        ));
        let value: serde_json::Value =
            serde_json::from_str(std::fs::read_to_string(log).unwrap().trim()).unwrap();
        assert_eq!(value["status"], "completed");
    }
    let comparison: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("results/sweep-comparison/comparison.json")).unwrap(),
    )
    .unwrap();
    let benchmarks = comparison["benchmarks"].as_array().unwrap();
    assert_eq!(benchmarks.len(), 8);
    for benchmark in benchmarks {
        assert_eq!(benchmark["status"], "completed", "{benchmark}");
        assert!(
            benchmark["metrics"]
                .as_object()
                .is_some_and(|m| !m.is_empty()),
            "{benchmark}"
        );
    }
    assert_eq!(
        std::fs::read_dir(&cassette_dir).unwrap().count(),
        1,
        "tape session files were cleaned up"
    );
    let default_dir = root.join("sweep-results/000-benchmark/cassettes");
    std::fs::create_dir_all(&default_dir).unwrap();
    std::fs::copy(&cassette, default_dir.join(cassette.file_name().unwrap())).unwrap();
    let result = asb(&root, &["sweep", "sweep.toml", "--replay"]);
    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}
