//! Observed-red fixtures for the old raw installer API. No production repair.
//!
//! Every raw call runs inside a separately admitted actual ASB guardian. The
//! parent watchdog owns that gate, its complete descendant tree and a distinct
//! sentinel gate. Failure assertions run only after matching cleanup receipts.
use super::*;
use crate::launch::ProcessLaunch;
use crate::process_gate::{Admission, ManagedGate};
use std::time::{Duration, Instant};

const CHILD: &str = "ASB_INSTALL_BOUNDARY_RED_CHILD";
const OBSERVATION: Duration = Duration::from_millis(500);
const INSPECTION_BYTES: usize = 4096;

struct Harness {
    root: PathBuf,
    gate: Option<ManagedGate>,
    sentinel: Option<ManagedGate>,
}
impl Harness {
    fn new(case: &str, test: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/installer-process-red")
            .join(crate::workloads::TaskIdentity::local(case, None).attempt_id);
        fs::create_dir_all(root.join(".asb")).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(
            root.join(".asb/project.toml"),
            "tools_dir='tools'\nworkloads_dir='workloads'\nworkspace_dir='workspace'\n",
        )
        .unwrap();
        fs::write(root.join("helper.py"), HELPER).unwrap();
        let binary = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("asb");
        let python = crate::tools::host_executable(Path::new("python3")).unwrap();
        let mut owner = Self {
            root,
            gate: None,
            sentinel: None,
        };
        let sentinel = ProcessLaunch::new("/bin/sleep");
        let (mut command, route) = sentinel.native(Some(&binary));
        command
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut sentinel = Admission::new(route.validate_gate(&mut command).unwrap())
            .unwrap()
            .spawn()
            .unwrap();
        sentinel.resume().unwrap();
        owner.sentinel = Some(sentinel);
        let mut launch = ProcessLaunch::new(std::env::current_exe().unwrap());
        launch
            .arg("--exact")
            .arg(test)
            .arg("--nocapture")
            .arg("--test-threads=1");
        let (mut command, route) = launch.native(Some(&binary));
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &owner.root)
            .env(CHILD, case)
            .env("ASB_INSTALL_BOUNDARY_PYTHON", python)
            .current_dir(&owner.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut gate = Admission::new(route.validate_gate(&mut command).unwrap())
            .unwrap()
            .spawn()
            .unwrap();
        gate.resume().unwrap();
        owner.gate = Some(gate);
        owner
    }
    fn await_file(&self, name: &str, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            if self.root.join(name).is_file() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.root.join(name).is_file()
    }
    fn settle(&mut self) {
        let main = self.gate.as_mut().unwrap().stop();
        let alive = self
            .sentinel
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none();
        let sentinel = self.sentinel.as_mut().unwrap().stop();
        assert!(
            main.is_ok(),
            "watchdog did not confirm owned tree cleanup: {main:?}; preserve {}",
            self.root.display()
        );
        assert!(
            sentinel.is_ok(),
            "sentinel cleanup unconfirmed: {sentinel:?}"
        );
        assert!(alive, "raw-call cleanup killed the unrelated sentinel");
        fs::write(
            self.root.join("watchdog-settlement"),
            "matching guardian exit and quiescence; unrelated sentinel survived\n",
        )
        .unwrap();
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        for gate in [&mut self.gate, &mut self.sentinel].into_iter().flatten() {
            let _ = gate.stop();
        }
        // Preserve all private fixture evidence, including unknown cleanup.
        // ManagedGate retains any unconfirmed guardian in wait-only custody.
    }
}

const HELPER: &str = r#"import os,sys,time
from pathlib import Path
p=Path.cwd();kind=sys.argv[1]
if kind == 'overflow':
 p.joinpath('helper-ready').touch()
 sys.stdout.buffer.write(b'x'*(64*1024));sys.stdout.flush()
 raise SystemExit(0)
if kind == 'stuck':
 p.joinpath('helper-ready').touch()
 while True:time.sleep(.02)
if kind == 'mutating':p.joinpath('stage').write_text('initial')
if kind == 'pipes':
 sys.stdout.write('valid-prefix');sys.stdout.flush()
pid=os.fork()
if pid == 0:
 p.joinpath('helper-ready').touch()
 while not p.joinpath('release-writer').exists():time.sleep(.005)
 if kind == 'mutating':
  p.joinpath('stage').write_text('mutated-after-raw-return')
  p.joinpath('writer-mutated').touch()
 while True:time.sleep(.02)
else:
 while not p.joinpath('helper-ready').exists():time.sleep(.005)
 p.joinpath('principal-exiting-zero').touch()
 os._exit(0)
"#;

fn child(case: &str) -> bool {
    if std::env::var(CHILD).ok().as_deref() != Some(case) {
        return false;
    }
    let python = PathBuf::from(std::env::var_os("ASB_INSTALL_BOUNDARY_PYTHON").unwrap());
    let root = std::env::current_dir().unwrap();
    let helper = root.join("helper.py");
    let value = match case {
        "stuck" | "mutating" => {
            let result = Command::new(python).arg(&helper).arg(case).logged_status();
            serde_json::json!({"returned":true,"error":result.is_err(),"success":result.as_ref().is_ok_and(|status|status.success())})
        }
        "pipes" | "overflow" => {
            let result = command_output(
                &python,
                &[helper.as_os_str(), std::ffi::OsStr::new(case)],
                "owned red fixture",
            );
            serde_json::json!({"returned":true,"error":result.is_err(),"bytes":result.as_ref().map(String::len).ok()})
        }
        _ => unreachable!(),
    };
    fs::write(
        root.join(".returned.tmp"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    fs::rename(root.join(".returned.tmp"), root.join("returned.json")).unwrap();
    // Stay alive until the explicit watchdog settles the whole tree, so an
    // outer normal exit cannot hide the legacy orphan-writer race.
    loop {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn status_step_requires_a_bounded_wait_and_owned_cleanup() {
    if child("stuck") {
        return;
    }
    let mut owner = Harness::new(
        "stuck",
        "install::process_boundary_red::status_step_requires_a_bounded_wait_and_owned_cleanup",
    );
    assert!(
        owner.await_file("helper-ready", Duration::from_secs(5)),
        "fixture helper did not start: {}",
        owner.root.display()
    );
    let returned = owner.await_file("returned.json", OBSERVATION);
    owner.settle();
    assert!(
        returned,
        "legacy LoggedStatus waited beyond the fixture deadline; owned watchdog cleaned the actual stuck helper: {}",
        owner.root.display()
    );
}

#[test]
fn status_return_must_not_leave_a_descendant_writer_mutating_staging() {
    if child("mutating") {
        return;
    }
    let mut owner = Harness::new(
        "mutating",
        "install::process_boundary_red::status_return_must_not_leave_a_descendant_writer_mutating_staging",
    );
    assert!(
        owner.await_file("helper-ready", Duration::from_secs(5)),
        "fixture did not start"
    );
    assert!(
        owner.await_file("returned.json", Duration::from_secs(2)),
        "fixture status did not return"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(owner.root.join("returned.json")).unwrap()).unwrap();
    assert_eq!(
        result["success"], true,
        "the raw direct principal must have returned a real exit zero"
    );
    assert_eq!(
        fs::read_to_string(owner.root.join("stage")).unwrap(),
        "initial"
    );
    fs::write(owner.root.join("release-writer"), "release").unwrap();
    let mutated = owner.await_file("writer-mutated", OBSERVATION);
    owner.settle();
    assert!(
        !mutated,
        "raw status succeeded before descendant quiescence; owned writer changed staging after return: {}",
        owner.root.display()
    );
}

#[test]
fn inspection_must_bound_inherited_pipe_lifetime_after_principal_exit() {
    if child("pipes") {
        return;
    }
    let mut owner = Harness::new(
        "pipes",
        "install::process_boundary_red::inspection_must_bound_inherited_pipe_lifetime_after_principal_exit",
    );
    assert!(
        owner.await_file("helper-ready", Duration::from_secs(5)),
        "fixture did not start"
    );
    let returned = owner.await_file("returned.json", OBSERVATION);
    owner.settle();
    assert!(
        returned,
        "raw command_output waited for an orphan's inherited pipe, not its exited principal: {}",
        owner.root.display()
    );
}

#[test]
fn inspection_must_reject_output_exceeding_its_declared_byte_budget() {
    if child("overflow") {
        return;
    }
    let mut owner = Harness::new(
        "overflow",
        "install::process_boundary_red::inspection_must_reject_output_exceeding_its_declared_byte_budget",
    );
    assert!(
        owner.await_file("helper-ready", Duration::from_secs(5)),
        "fixture did not start"
    );
    assert!(
        owner.await_file("returned.json", Duration::from_secs(2)),
        "finite overflow helper did not return"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(owner.root.join("returned.json")).unwrap()).unwrap();
    owner.settle();
    assert_eq!(
        result["error"],
        true,
        "legacy command_output accepted {} bytes with no way to enforce the fixture's {INSPECTION_BYTES}-byte budget: {}",
        result["bytes"],
        owner.root.display()
    );
}
