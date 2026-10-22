//! Actual task/controller cancellation with inert, guardian-owned host children.
#![allow(unsafe_code)] // Pinned exact guardian identity/exit/reaping observations.
use super::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;

fn exited(identity: &OwnedFd) -> bool {
    let mut poll = libc::pollfd {fd:identity.as_raw_fd(), events:libc::POLLIN, revents:0};
    unsafe { libc::poll(&mut poll, 1, 0) > 0 && poll.revents & libc::POLLIN != 0 }
}

fn was_reaped(identity: &OwnedFd) -> bool {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe { libc::waitid(libc::P_PIDFD, identity.as_raw_fd() as libc::id_t,
        info.as_mut_ptr(), libc::WEXITED | libc::WNOHANG | libc::WNOWAIT) };
    result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD)
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/material-transport-custody.py")
}

fn child_case(root: &Path, scenario: &str) {
    let custody = crate::process_cleanup::test_support::Scope::new();
    let mut command = ControllerCommand::new("python3").unwrap();
    command
        .args(["-I", "-S", "-B", "-u"])
        .arg(fixture())
        .arg("controller")
        .arg(root)
        .arg(scenario);
    let mut controller = Controller::spawn(&mut command).unwrap();
    let gate_pid = controller.child().id();
    let image = std::fs::metadata(format!("/proc/{gate_pid}/exe")).unwrap();
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, gate_pid, 0) } as i32;
    assert!(descriptor >= 0);
    let gate_identity = unsafe { OwnedFd::from_raw_fd(descriptor) };
    std::fs::write(root.join("gate.json"),serde_json::to_vec(&json!({
        "pid":gate_pid,"image":[image.dev(), image.ino()]
    })).unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    assert_eq!(controller.receive(deadline).unwrap()["event"], "ready");
    controller
        .send(&json!({"op":"initial-material"}), deadline)
        .unwrap();
    let error = controller.receive(deadline).unwrap_err();
    let short = matches!(scenario,"retained"|"ownerloss");
    let missing = scenario == "missing-proof";
    if short {
        // Establish the actual journal-owned preservation independently before
        // starting the deliberately short host-transport settlement budget.
        // This uses the real protocol reader/context exit, not a fake CLOSED.
        controller.send(&json!({"op":"cancel"}),deadline).unwrap();
        let preserved = Instant::now()+Duration::from_secs(5);
        while !root.join("interruption.json").is_file() {
            assert!(Instant::now()<preserved,"fixture preservation handshake deadline");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    if missing { controller.lose_receipt_for_test(); }
    // Existing production cancellation allowance is240s. Six seconds here
    // includes one aggregate3s Python close plus actual guardian quiescence;
    // the independent40ms fixture below preserves the original deadline test.
    let cancelled = controller.cancel(if short { Duration::from_millis(40) }
                                     else { Duration::from_secs(6) });
    if short || missing {
        let uncertainty = cancelled.unwrap_err();
        assert!(uncertainty.contains("retains custody"),"{uncertainty}");
        let alive_at_return = !exited(&gate_identity);
        if short { assert!(alive_at_return,"fixture must observe a genuinely retained live gate"); }
        std::fs::write(root.join("controller.json"),serde_json::to_vec(&json!({
            "error":error,"cancel_confirmed":false,"retained_alive":alive_at_return,
            "gate_pid":gate_pid,"uncertainty":uncertainty
        })).unwrap()).unwrap();
        if scenario == "ownerloss" {
            // Actual owner death, no destructor/keeper completion fiction.
            // Outer guardian releases actors, adopts, waits and reaps real gate.
            std::process::exit(0);
        }
        std::fs::write(root.join("request-release"),b"release exact fixture actors").unwrap();
        custody.wait_finished();
        let gate_reaped = exited(&gate_identity) && was_reaped(&gate_identity);
        std::fs::write(root.join("gate-settled.json"),serde_json::to_vec(&json!({
            "actual_exit_and_reap":gate_reaped,"keeper_count":custody.count()
        })).unwrap()).unwrap();
        assert!(gate_reaped);
        drop(controller);
        return;
    }
    assert_eq!(cancelled.unwrap(), ResourceDisposition::StoppedPreserved);
    assert!(exited(&gate_identity) && was_reaped(&gate_identity));
    std::fs::write(
        root.join("controller.json"),
        serde_json::to_vec(&json!({"error":error,"preserved":true,"cancel_confirmed":true})).unwrap(),
    )
    .unwrap();
    drop(controller);
    custody.wait_finished();
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
    assert_eq!(guardian["cleanup_error"],Value::Null);
    assert_eq!(guardian["adopted_gate_kills"],0);
    assert_eq!(read("reloaded.json")["running"], false);
    assert!(root.join("interruption.json").is_file());
    let remaining = guardian["adopted_alive_after_controller_exit"]
        .as_array()
        .unwrap();
    let observed=read("controller.json");
    if scenario=="ownerloss" {
        assert_eq!(observed["cancel_confirmed"],false);
        assert_eq!(observed["retained_alive"],true);
        assert!(guardian["adopted_actual_gates"].as_array().unwrap().contains(&observed["gate_pid"]));
        assert!(guardian["actually_reaped"].as_array().unwrap().contains(&observed["gate_pid"]));
        return;
    }
    if matches!(scenario,"retained"|"missing-proof") {
        assert_eq!(observed["cancel_confirmed"],false);
        assert_eq!(read("gate-settled.json")["actual_exit_and_reap"],true);
        assert_eq!(read("gate-settled.json")["keeper_count"],1);
    } else {
        assert_eq!(observed["preserved"],true);
        assert_eq!(observed["cancel_confirmed"],true);
    }
    if scenario == "inherited" {
        let workers = read("workers.json");
        println!("live-session workers={workers}");
        assert_eq!(
            workers["live_after_error"], 0,
            "failed material exchange returned with live pipe workers; guardian cleanup already complete"
        );
    } else {
        assert_eq!(read("transport-error.json"), "DockerAccessError");
        assert_eq!(read("connections.json").as_array().unwrap().len(), 3);
    }
    assert!(
        remaining.is_empty(),
        "confirmed guest preservation/controller exit left host children outside retained custody; guardian cleanup already complete"
    );
}

#[test]
fn short_deadline_retains_actual_gate_until_owned_release_and_keeper_reap() {
    whole_path("short_deadline_retains_actual_gate_until_owned_release_and_keeper_reap","retained");
}

#[test]
fn actual_owner_loss_keeps_gate_alive_until_release_and_guardian_adoption_reap() {
    whole_path("actual_owner_loss_keeps_gate_alive_until_release_and_guardian_adoption_reap","ownerloss");
}

#[test]
fn missing_quiescence_proof_never_becomes_confirmed_resource_settlement() {
    whole_path("missing_quiescence_proof_never_becomes_confirmed_resource_settlement","missing-proof");
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
