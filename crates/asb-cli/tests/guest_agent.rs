//! Offline guest-driver checks using a registered adapter and fixture executable.
#![cfg(unix)]
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "asb-guest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for directory in [".asb", ".asb-tools/pi/bin", "workspace"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        std::fs::write(
            root.join(".asb/project.toml"),
            "tools_dir='.asb-tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n",
        )
        .unwrap();
        let program = root.join(".asb-tools/pi/bin/pi");
        std::fs::write(
            &program,
            r#"#!/bin/sh
test -z "$ASB_GUEST_AGENT_REQUEST" || exit 90
test -z "$CODEX_REMOTE_PAYLOAD" || exit 91
test "$ASB_AGENT_API_KEY" = fixture-token || exit 92
printf 'changed\n' > performed.txt
printf '%s\n' '{"type":"turn_end","usage":{"input":17,"output":5,"cacheRead":0}}'
printf '%s\n' '{"type":"result","result":"fixture completed"}'
printf 'fixture diagnostic\n' >&2
exit 7
"#,
        )
        .unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(root)
    }
    fn request(&self) -> serde_json::Value {
        serde_json::json!({
            "config": {"agent":"pi", "provider":"openrouter", "model":"fixture-model", "workload":"fixture", "workspace":self.0.join("workspace")},
            "connection": {"interface":"open_ai_chat", "authentication":"bearer", "base_url":"http://127.0.0.1:1/v1", "credential":"fixture-token"},
            "prompt":"offline fixture task", "project_root":self.0,
            "runtime_dir":self.0.join("private"), "run_dir":self.0.join("private/attempt"), "replay":false,
        })
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn guest_uses_registered_adapter_prepares_before_gate_and_preserves_output_usage_and_failure() {
    let project = Project::new();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_asb"))
            .arg("__asb-agent")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", &project.0)
            .env("ASB_GUEST_AGENT_REQUEST", project.request().to_string())
            .env("CODEX_REMOTE_PAYLOAD", "must-not-leak")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = BufReader::new(process.0.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    assert_eq!(first.trim(), format!("__ASB_GATE__:{}", process.0.id()));
    assert!(project.0.join("private/pi/config").is_dir());
    assert!(!project.0.join("workspace/performed.txt").exists());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !std::fs::read_to_string(format!("/proc/{}/status", process.0.id()))
        .unwrap()
        .lines()
        .any(|line| line.starts_with("State:") && line.contains('T'))
    {
        assert!(
            Instant::now() < deadline,
            "guest never reached stopped state"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        Command::new("kill")
            .args(["-CONT", &process.0.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let mut text = String::new();
    stdout.read_to_string(&mut text).unwrap();
    let mut error = String::new();
    process
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut error)
        .unwrap();
    assert_eq!(process.0.wait().unwrap().code(), Some(7), "{error}");
    assert!(text.contains("fixture completed"));
    assert!(!text.contains("fixture diagnostic"));
    assert!(error.contains("fixture diagnostic"));
    assert!(project.0.join("workspace/performed.txt").is_file());
    let bytes = std::fs::read(project.0.join("private/attempt/agent-result.json")).unwrap();
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["exit_code"], 7);
    assert_eq!(result["token_usage"]["input_tokens"], 17);
    assert_eq!(result["token_usage"]["output_tokens"], 5);
    assert_eq!(result["token_usage"]["total_tokens"], 22);
    assert_eq!(result["final_response"], "fixture completed");
    assert!(!String::from_utf8_lossy(&bytes).contains("fixture-token"));
}

#[test]
fn malformed_guest_request_fails_without_echoing_input_or_starting_an_agent() {
    let output = Command::new(env!("CARGO_BIN_EXE_asb"))
        .arg("__asb-agent")
        .env_clear()
        .env("ASB_GUEST_AGENT_REQUEST", "not-json-sensitive-fixture")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive-fixture"));
}
