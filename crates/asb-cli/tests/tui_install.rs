//! Real CLI, kernel pipe failure and owned offline installer descendants.
#![cfg(target_os = "linux")]
mod support;
use std::process::Command;
use support::IsolatedEnvironment;

#[test]
fn process_backed_installation_survives_presentation_failure_without_orphans() {
    let output = Command::new("python3")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/tui_install.py"))
        .arg(env!("CARGO_BIN_EXE_asb"))
        .isolated_env()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
