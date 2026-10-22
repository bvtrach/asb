//! Test-only outer process custody. Never terminate an admitted guardian.
use crate::process_cleanup::OwnedReaper;
use crate::test_support::IsolatedEnvironment;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Explicit images survive frozen libtest reexec outside target/debug/deps.
pub(crate) fn actual_asb() -> Result<PathBuf, String> {
    let candidate = if let Some(path) = std::env::var_os("ASB_RUNTIME_EXECUTABLE") {
        PathBuf::from(path)
    } else if let Some(target) = option_env!("CARGO_TARGET_DIR") {
        Path::new(target).join("debug/asb")
    } else {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = if manifest.ends_with("crates/asb-cli") {
            manifest
                .parent()
                .and_then(Path::parent)
                .ok_or("invalid workspace fixture root")?
        } else {
            manifest
        };
        root.join("target/debug/asb")
    };
    let image = candidate.canonicalize().map_err(
        |_| "build actual ASB first or provide its explicit ASB_RUNTIME_EXECUTABLE path",
    )?;
    if !image.is_file() {
        return Err("actual ASB image is not a regular file".into());
    }
    Ok(image)
}

struct Guardian {
    child: Option<Child>,
    keeper: Option<OwnedReaper>,
    namespace: PathBuf,
}

impl Guardian {
    fn spawn(command: &mut Command, namespace: &Path) -> Result<Self, String> {
        let keeper = OwnedReaper::new("material-fixture-guardian")?;
        let mut owner = Self {
            child: None,
            keeper: Some(keeper),
            namespace: namespace.to_owned(),
        };
        // Immediate ownership after spawn; no callback or fallible publication
        // is allowed between receiving Child and installing this guard.
        owner.child = Some(
            command
                .spawn()
                .map_err(|error| format!("cannot start fixture guardian: {error}"))?,
        );
        Ok(owner)
    }

    fn retain(&mut self, reason: &str) -> String {
        if let Some(child) = self.child.take() {
            self.keeper
                .as_ref()
                .expect("keeper predates spawn")
                .adopt_wait_only(child);
            self.keeper.take();
        }
        format!(
            "{reason}; fixture guardian remains in wait-only custody; retain namespace {}",
            self.namespace.display()
        )
    }

    fn wait(&mut self, deadline: Instant) -> Result<ExitStatus, String> {
        loop {
            match self
                .child
                .as_mut()
                .expect("owned fixture guardian")
                .try_wait()
            {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok(status);
                }
                Err(_) => return Err(self.retain("cannot observe fixture guardian exit")),
                Ok(None) => {}
            }
            if self.namespace.join("guardian-error.json").exists() {
                return Err(
                    self.retain("fixture cleanup reported uncertainty, not completed settlement")
                );
            }
            if Instant::now() >= deadline {
                return Err(self.retain("fixture guardian exceeded caller deadline"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Guardian {
    fn drop(&mut self) {
        self.retain("fixture owner unwound");
    }
}

fn spawn_fixture(
    root: &Path,
    test: &str,
    scenario: &str,
    control: &str,
) -> Result<Guardian, String> {
    let actual = actual_asb()?;
    let mut command = Command::new("python3");
    command
        .isolated_env()
        .env("PATH", "/usr/bin:/bin")
        .env("ASB_RUNTIME_EXECUTABLE", &actual)
        .args(["-I", "-S", "-B"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/material-transport-custody.py"),
        )
        .arg("guardian")
        .arg(root)
        .arg(std::env::current_exe().map_err(|error| error.to_string())?)
        .arg(test)
        .arg(scenario)
        .arg(&actual)
        .arg(control)
        .stdin(Stdio::null());
    for (name, stdout) in [("guardian.stdout", true), ("guardian.stderr", false)] {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(name))
            .map_err(|error| error.to_string())?;
        if stdout {
            command.stdout(file);
        } else {
            command.stderr(file);
        }
    }
    Guardian::spawn(&mut command, root)
}

pub(crate) fn run(root: &Path, test: &str, scenario: &str) -> Result<ExitStatus, String> {
    let mut guardian = spawn_fixture(root, test, scenario, "normal")?;
    guardian.wait(Instant::now() + Duration::from_secs(55))
}

#[cfg(test)]
mod tests {
    #![allow(unsafe_code)] // Exact pidfd exit/reap observations, never PID signals.
    use super::*;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    struct Release(PathBuf);
    impl Release {
        fn release(&self) -> std::io::Result<()> {
            std::fs::write(
                self.0.join("release-fixtures"),
                b"release exact fixture actors",
            )
        }
    }
    impl Drop for Release {
        fn drop(&mut self) {
            // Best-effort unwind safeguard; explicit failure is reported below,
            // while the actual guardian stays in wait-only custody.
            let _ = self.release();
        }
    }
    fn pin(pid: u32) -> Result<OwnedFd, String> {
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
    fn exited(fd: &OwnedFd) -> bool {
        let mut event = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        unsafe { libc::poll(&mut event, 1, 0) > 0 && event.revents & libc::POLLIN != 0 }
    }
    fn reaped(fd: &OwnedFd) -> bool {
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PIDFD,
                fd.as_raw_fd() as libc::id_t,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD)
    }
    fn read(root: &Path, name: &str) -> Result<serde_json::Value, String> {
        serde_json::from_slice(&std::fs::read(root.join(name)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    fn wait_marker(root: &Path, name: &str) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join(name).is_file() {
            if Instant::now() >= deadline {
                return Err(format!("missing owned fixture marker {name}"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
    fn case(kind: &str) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/material50-fixtures")
            .join(crate::workloads::TaskIdentity::local(kind, None).attempt_id);
        std::fs::create_dir_all(&root).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let receipts = crate::process_cleanup::test_support::Scope::new();
        // Outlives both the owned guardian and the controlled unwind closure.
        let release = Release(root.clone());
        let mut guardian_pin = None;
        let mut gate_pin = None;
        let observations = (|| -> Result<(String, bool, bool), String> {
            let control = if kind == "report" {
                "short-report"
            } else {
                "deferred"
            };
            let mut owner = Some(spawn_fixture(
                &root,
                "workloads::controller::material_custody_tests::actual_owner_loss_keeps_gate_alive_until_release_and_guardian_adoption_reap",
                "ownerloss",
                control,
            )?);
            guardian_pin = Some(pin(owner.as_ref().unwrap().child.as_ref().unwrap().id())?);
            wait_marker(&root, "adopted-live-gate.json")?;
            let pid = read(&root, "adopted-live-gate.json")?["pid"]
                .as_u64()
                .ok_or("missing exact adopted gate identity")? as u32;
            gate_pin = Some(pin(pid)?);
            let reason = if kind == "unwind" {
                let owned = owner.take().unwrap();
                let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _owned = owned;
                    panic!("controlled outer fixture owner unwind");
                }));
                if unwind.is_ok() {
                    return Err("controlled unwind did not occur".into());
                }
                "controlled unwind".to_owned()
            } else {
                let budget = if kind == "report" {
                    Duration::from_secs(2)
                } else {
                    Duration::from_millis(40)
                };
                owner
                    .as_mut()
                    .unwrap()
                    .wait(Instant::now() + budget)
                    .err()
                    .ok_or("guardian unexpectedly completed before owned release")?
            };
            let still_live =
                !exited(guardian_pin.as_ref().unwrap()) && !exited(gate_pin.as_ref().unwrap());
            let premature = receipts.finished();
            drop(owner);
            Ok((reason, still_live, premature))
        })();
        // No intended assertions before explicit release + actual settlement.
        let released = release.release();
        let deadline = Instant::now() + Duration::from_secs(6);
        while !receipts.finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        let settled = receipts.finished();
        let outer_reaped = guardian_pin
            .as_ref()
            .is_some_and(|fd| exited(fd) && reaped(fd));
        let gate_exited = gate_pin.as_ref().is_some_and(exited);
        let final_record = read(&root, "guardian.json");
        println!(
            "fixture={} kind={kind} guardian={final_record:?} settled={settled}",
            root.display()
        );
        assert!(
            released.is_ok(),
            "release failed; namespace retained {}",
            root.display()
        );
        assert!(
            settled && outer_reaped,
            "guardian retained; inspect {}",
            root.display()
        );
        let (reason, still_live, premature) = observations.unwrap();
        let record = final_record.unwrap();
        assert!(still_live && !premature && gate_exited);
        assert_eq!(receipts.count(), 1);
        assert_eq!(record["sentinel_survived"], true);
        assert_eq!(record["adopted_gate_kills"], 0);
        assert_eq!(record["child_status"], 0);
        let pid = read(&root, "adopted-live-gate.json").unwrap()["pid"].clone();
        assert!(record["actually_reaped"].as_array().unwrap().contains(&pid));
        assert!(root.is_dir());
        if kind == "report" {
            assert!(
                reason.contains("reported uncertainty")
                    && reason.contains(&root.display().to_string())
            );
            assert_eq!(record["cleanup_error"], "CleanupDeadlineUnknown");
            assert_eq!(
                read(&root, "guardian-error.json").unwrap()["custody"],
                "retained-not-settled"
            );
        } else {
            assert_eq!(record["cleanup_error"], serde_json::Value::Null);
            if kind == "timeout" {
                assert!(
                    reason.contains("caller deadline")
                        && reason.contains(&root.display().to_string())
                );
            }
        }
    }
    #[test]
    fn finite_python_unknown_report_retains_outer_guardian_until_actual_release_and_reap() {
        case("report");
    }
    #[test]
    fn caller_timeout_retains_outer_guardian_until_actual_release_and_reap() {
        case("timeout");
    }
    #[test]
    fn caller_unwind_retains_outer_guardian_until_actual_release_and_reap() {
        case("unwind");
    }
}
