//! Actual Source entry composition; historical raw-Child faults remain in Git.
use super::*;
use crate::launch::ProcessLaunch;
use crate::process_gate::{Admission, ManagedGate};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

struct Sentinel(Option<ManagedGate>);
impl Sentinel {
    fn start() -> Self {
        let binary = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("asb");
        let (mut command, route) = ProcessLaunch::new("/bin/sleep").native(Some(&binary));
        command
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut gate = Admission::new(route.validate_gate(&mut command).unwrap())
            .unwrap()
            .spawn()
            .unwrap();
        gate.resume().unwrap();
        Self(Some(gate))
    }
    fn alive(&mut self) -> bool {
        self.0.as_mut().unwrap().try_wait().unwrap().is_none()
    }
    fn settle(&mut self) -> bool {
        self.0.take().unwrap().stop().is_ok()
    }
}
impl Drop for Sentinel {
    fn drop(&mut self) {
        if let Some(mut gate) = self.0.take() {
            let _ = gate.stop();
        }
    }
}
struct Cancel(OperationContext);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct Unhold(Arc<AtomicBool>);
impl Drop for Unhold {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn wait_for(mut observed: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(5);
    while !observed() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    observed()
}
fn options() -> Options<'static> {
    Options {
        kind: "actual Source lifecycle fixture",
        limit: 4096,
        checksum: None,
        executable: true,
    }
}

#[test]
fn actual_source_lifecycle_preserves_abort_and_ordinary_resource_contracts() {
    for case in [
        "download-worker-start",
        "caught-abort-ok",
        "caught-abort-ordinary",
        "unwind-after-abort",
        "normal-success",
    ] {
        publication_tests::Fixture::new().run(&format!("lifecycle-{case}"));
    }
}

pub(super) fn run_case(root: &Path, destination: &Path, case: &str) {
    let case = case.strip_prefix("lifecycle-").unwrap();
    let context = OperationContext::load(root).unwrap();
    let mut sentinel = Sentinel::start();
    if case == "download-worker-start" {
        // Only this operation's next Download preparation is affected. If the
        // hook regresses, the inherited fixture PATH still selects inert curl.
        let observed = context.fail_next_worker_for_test(Purpose::Download);
        let mut validated = false;
        let result = install_input_in(
            &context,
            &Input::Remote("https://fixture.invalid/success".into()),
            destination,
            options(),
            |_, _| {
                validated = true;
                Ok(())
            },
            Publication::Replace,
        );
        let (raw_directory, completion) = observed.recv_timeout(Duration::from_secs(1)).unwrap();
        let dropped = completion.load(Ordering::Acquire);
        let alive = sentinel.alive();
        let settled = sentinel.settle();
        assert!(alive && settled);
        assert!(dropped, "actual never-admitted Stage destructor is pending");
        let Err(InstallFailure::Ordinary(error)) = result else {
            panic!("pre-admission failure lost ordinary classification");
        };
        assert!(error.to_string().contains("custody worker"), "{error}");
        assert!(
            !validated && !root.join("download-path").exists(),
            "downloader or validator was admitted"
        );
        assert!(
            !raw_directory.exists(),
            "known-never-admitted Stage cleanup was withheld"
        );
        publication_tests::assert_old_ready(root, destination);
        return;
    }
    if case == "normal-success" {
        let mut owner = None;
        let result = install_input_in(
            &context,
            &Input::pin_local(root.join("new-source")).unwrap(),
            destination,
            options(),
            |path, scope| {
                owner = Some((path.to_owned(), scope.fixture_drop_completion().unwrap()));
                check_version_in(scope, path, "healthy lifecycle fixture")
            },
            Publication::Replace,
        );
        let (raw, completion) = owner.unwrap();
        let dropped = completion.load(Ordering::Acquire);
        let alive = sentinel.alive();
        let settled = sentinel.settle();
        assert!(alive && settled);
        result.unwrap();
        assert!(dropped && !raw.exists());
        assert_eq!(
            fs::read(destination).unwrap(),
            b"#!/bin/sh\nprintf 'new tool\\n'\n"
        );
        return;
    }
    enum Outcome {
        Returned(InstallResult<()>),
        Unwound,
    }
    let held = Arc::new(AtomicBool::new(true));
    let release = Unhold(Arc::clone(&held));
    let _cancel = Cancel(context.clone());
    let (resources, receipt) = mpsc::sync_channel(1);
    let (send, receive) = mpsc::sync_channel(1);
    let worker_context = context.clone();
    let worker_root = root.to_owned();
    let worker_destination = destination.to_owned();
    let worker_case = case.to_owned();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            install_input_in(
                &worker_context,
                &Input::pin_local(worker_root.join("new-source")).unwrap(),
                &worker_destination,
                options(),
                |path, scope| {
                    let _ =
                        resources.send((path.to_owned(), scope.fixture_drop_completion().unwrap()));
                    let mut step = scope.prepare(
                        "/bin/sh",
                        Purpose::Version,
                        "uncertain lifecycle producer",
                    )?;
                    step.command_mut()
                        .current_dir(&worker_root)
                        .env("PATH", "/usr/bin:/bin")
                        .args(["-c", "printf ready > lifecycle-ready; sleep 30"])
                        .stdin(Stdio::null());
                    step.hold_observation_for_test(held);
                    let failure = scope.run(step);
                    // The caller may catch its step failure, but cannot erase
                    // the same scope's sticky Abort or mint settlement proof.
                    if !matches!(failure, Err(InstallFailure::Abort(_))) {
                        return Err(
                            Error::Input("fixture producer did not return Abort".into()).into()
                        );
                    }
                    match worker_case.as_str() {
                        "caught-abort-ok" => Ok(()),
                        "caught-abort-ordinary" => {
                            Err(Error::Input("specific lifecycle primary reason".into()).into())
                        }
                        "unwind-after-abort" => {
                            panic!("static fixture unwind after uncertain producer")
                        }
                        _ => unreachable!(),
                    }
                },
                Publication::Replace,
            )
        }));
        let outcome = match result {
            Ok(result) => Outcome::Returned(result),
            Err(_) => Outcome::Unwound,
        };
        let _ = send.send(outcome);
    });
    let (raw, completion) = receipt.recv_timeout(Duration::from_secs(5)).unwrap();
    let ready = wait_for(|| root.join("lifecycle-ready").is_file());
    context.cancel();
    let outcome = receive.recv_timeout(Duration::from_secs(8)).unwrap();
    let retained_before = raw.is_file();
    let held_before = !completion.load(Ordering::Acquire);
    let alive_before = sentinel.alive();
    drop(release); // Only now allow matching guardian observation/owner release.
    let dropped = wait_for(|| completion.load(Ordering::Acquire));
    let alive_after = sentinel.alive();
    let settled = sentinel.settle();
    // All resource and outcome assertions follow explicit owned settlement.
    assert!(ready && alive_before && alive_after && settled);
    assert!(held_before && retained_before && dropped);
    assert!(
        raw.is_file(),
        "late actual Stage Drop deleted retained evidence"
    );
    publication_tests::assert_old_ready(root, destination);
    match (case, outcome) {
        ("unwind-after-abort", Outcome::Unwound) => {}
        ("caught-abort-ok", Outcome::Returned(Err(InstallFailure::Abort(_)))) => {}
        ("caught-abort-ordinary", Outcome::Returned(Err(InstallFailure::Abort(error)))) => {
            let text = error.to_string();
            assert_eq!(text.matches("specific lifecycle primary reason").count(), 1);
        }
        _ => panic!("actual Source erased a caught Abort/unwind or allowed publication"),
    }
    fs::write(root.join("actual-source-settlement"), "actual Stage Drop completed after matching observation release; unrelated sentinel settled separately").unwrap();
}
