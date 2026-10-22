//! Actual task/controller cancellation with inert, guardian-owned host children.
use super::*;
use serde_json::json;
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/material-transport-custody.py")
}

fn child_case(root: &Path, scenario: &str) {
    let mut command = Command::new("python3");
    command
        .args(["-I", "-S", "-B", "-u"])
        .arg(fixture())
        .arg("controller")
        .arg(root)
        .arg(scenario);
    let mut controller = Controller::spawn(&mut command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    assert_eq!(controller.receive(deadline).unwrap()["event"], "ready");
    controller
        .send(&json!({"op":"initial-material"}), deadline)
        .unwrap();
    let error = controller.receive(deadline).unwrap_err();
    let disposition = controller.cancel(Duration::from_secs(3)).unwrap();
    assert_eq!(disposition, ResourceDisposition::StoppedPreserved);
    std::fs::write(
        root.join("controller.json"),
        serde_json::to_vec(&json!({"error":error,"preserved":true})).unwrap(),
    )
    .unwrap();
}

fn whole_path(test: &str, scenario: &str) {
    if let Some(selected) = std::env::var_os("ASB_MATERIAL_CUSTODY_CHILD") {
        assert_eq!(selected, scenario);
        child_case(
            &PathBuf::from(std::env::var_os("ASB_MATERIAL_CUSTODY_ROOT").unwrap()),
            scenario,
        );
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/material50-fixtures")
        .join(crate::workloads::TaskIdentity::local(scenario, None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let status = Command::new("python3")
        .args(["-I", "-S", "-B"])
        .arg(fixture())
        .arg("guardian")
        .arg(&root)
        .arg(std::env::current_exe().unwrap())
        .arg(format!(
            "workloads::controller::material_custody_tests::{test}"
        ))
        .arg(scenario)
        .status()
        .unwrap();
    let read = |name: &str| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(name)).unwrap()).unwrap()
    };
    let guardian = read("guardian.json");
    println!("fixture={} guardian={guardian}", root.display());
    assert!(status.success());
    assert_eq!(guardian["child_status"], 0);
    assert_eq!(guardian["sentinel_survived"], true);
    assert_eq!(read("controller.json")["preserved"], true);
    assert_eq!(read("reloaded.json")["running"], false);
    assert!(root.join("interruption.json").is_file());
    let remaining = guardian["adopted_alive_after_controller_exit"]
        .as_array()
        .unwrap();
    assert_eq!(
        guardian["actually_reaped"].as_array().unwrap().len(),
        remaining.len()
    );
    if scenario == "inherited" {
        let workers = read("workers.json");
        println!("live-session workers={workers}");
        assert_eq!(
            workers["live_after_error"], 0,
            "failed material exchange returned with live pipe workers; guardian cleanup already complete"
        );
    } else {
        assert_eq!(read("transport-error.json"), "PermissionError");
        assert_eq!(read("connections.json").as_array().unwrap().len(), 3);
    }
    assert!(
        remaining.is_empty(),
        "confirmed guest preservation/controller exit left host children outside retained custody; guardian cleanup already complete"
    );
}

#[test]
fn material_inherited_pipe_has_no_workers_or_host_children_after_preserving_cancel() {
    whole_path(
        "material_inherited_pipe_has_no_workers_or_host_children_after_preserving_cancel",
        "inherited",
    );
}

#[test]
fn first_transport_signal_denial_does_not_abandon_later_connections() {
    whole_path(
        "first_transport_signal_denial_does_not_abandon_later_connections",
        "denied",
    );
}
