//! Actual native/guest artifact boundaries; no packages, providers or host setup.
#![cfg(target_os = "linux")]
mod support;
use std::process::Command;
use support::IsolatedEnvironment;

#[test]
fn native_and_guest_artifacts_are_attempt_owned_and_ancillary_failures_keep_results() {
    let output = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/agent_artifact_ownership.py"
        ))
        .arg(env!("CARGO_BIN_EXE_asb"))
        .arg("--extended")
        .isolated_env()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
