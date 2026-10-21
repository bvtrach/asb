//! Actual native admission with entirely inert, offline fixture programs.
#![allow(unsafe_code)] // Observe pinned fixture identities using pidfd, never signal.
use super::*;
use std::fs;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Condvar, Mutex, mpsc};

fn root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/installer-process-core")
        .join(crate::tools::runtime::unique_id());
    fs::create_dir_all(&root).unwrap();
    fs::canonicalize(root).unwrap()
}
fn context(root: &Path) -> OperationContext {
    let config = ProjectConfig::default();
    OperationContext::new(root, &config).unwrap()
}
fn shell(context: &OperationContext, script: &str) -> PreparedStep {
    let mut step =
        PreparedStep::new(context, "/bin/sh", Purpose::Probe, "offline fixture").unwrap();
    step.command_mut()
        .args(["-c", script])
        .current_dir(&context.root)
        .stdin(std::process::Stdio::null());
    step
}
fn wait_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "missing handshake {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn purpose_budgets_share_the_frozen_operation_and_render_only_at_boundary() {
    let root = root();
    let mut config = ProjectConfig::default();
    config.installation.operation_timeout_seconds = 2;
    let context = OperationContext::new(&root, &config).unwrap();
    config.installation.operation_timeout_seconds = 100;
    for purpose in [
        Purpose::Probe,
        Purpose::Version,
        Purpose::Download,
        Purpose::Extract,
        Purpose::Configure,
        Purpose::Package,
        Purpose::Build,
    ] {
        let (end, key) = context.step_deadline(purpose).unwrap();
        assert_eq!(end, context.deadline);
        assert_eq!(key, "operation_timeout_seconds");
    }
    let shorter = OperationContext::new(&root, &config).unwrap();
    assert_eq!(
        shorter.step_deadline(Purpose::Version).unwrap().1,
        "version_timeout_seconds"
    );
    context.cancel();
    let failure = match shell(&context, "exit 0").run(ProducerLease::default()) {
        Err(failure) => failure,
        Ok(_) => panic!("cancelled operation admitted a target"),
    };
    assert!(matches!(failure, InstallFailure::Abort(_)));
    assert!(failure.into_public().to_string().contains("Cancelled"));
}

#[test]
fn actual_prepared_command_preserves_configuration_and_private_logs() {
    let root = root();
    let context = context(&root);
    let mut launch = crate::launch::ProcessLaunch::new("/bin/sh");
    launch.arg0("intentional-installer-name");
    let mut step =
        PreparedStep::from_launch(&context, &launch, Purpose::Probe, "configuration fixture")
            .unwrap();
    step.command_mut()
        .env_clear()
        .env("EXPLICIT", "kept")
        .current_dir(&root)
        .args([
            "-c",
            "printf '%s|%s|%s' \"$0\" \"$EXPLICIT\" \"$PWD\"; printf 'private-stderr' >&2",
        ])
        .stdin(std::process::Stdio::null());
    step.capture(Capture::inspection());
    let result = step.run(ProducerLease::default()).unwrap();
    assert!(result.status().success());
    assert_eq!(
        String::from_utf8(result.stdout().to_vec()).unwrap(),
        format!("intentional-installer-name|kept|{}", root.display())
    );
    assert_eq!(result.stderr(), b"private-stderr");
    let path = result.log.path.unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(result.log.accepted > 0);
    assert!(result.log.retained <= result.log.accepted);
    assert_eq!(result.log.discarded, 0);
    assert!(result.log.fault.is_none());
    let _settled_diagnostic = result.log.settled;
}

#[test]
fn actual_inspection_overflow_aborts_without_accepting_a_prefix() {
    let root = root();
    let context = context(&root);
    let mut step = shell(&context, "printf 'abcdefghijk'");
    step.capture(Capture::Inspect {
        stream_bytes: 4,
        total_bytes: 8,
    });
    match step.run(ProducerLease::default()) {
        Err(InstallFailure::Abort(abort)) => {
            assert_eq!(abort.cause, AbortCause::CaptureOverflow);
            assert!(abort.cleanup_confirmed);
            assert!(!abort.capture_complete);
        }
        _ => panic!("overflow was accepted or erased into an ordinary candidate failure"),
    }
}

struct HeldWriter {
    file: fs::File,
    reached: Arc<AtomicBool>,
    release: Arc<(Mutex<bool>, Condvar)>,
}
impl Write for HeldWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.reached.store(true, Ordering::Release);
        let (lock, wake) = &*self.release;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = wake.wait(released).unwrap();
        }
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
struct Release(Arc<(Mutex<bool>, Condvar)>);
impl Drop for Release {
    fn drop(&mut self) {
        let (lock, wake) = &*self.0;
        *lock.lock().unwrap() = true;
        wake.notify_all();
    }
}
struct Dependency(Arc<AtomicBool>);
impl Drop for Dependency {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn pin(pid: u32) -> fs::File {
    // SAFETY: pin a fixture PID only for poll observation, never signalling.
    let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(
        fd >= 0,
        "cannot observe owned fixture: {}",
        io::Error::last_os_error()
    );
    unsafe { fs::File::from_raw_fd(fd) }
}
fn exited(identity: &fs::File) -> bool {
    let mut descriptor = libc::pollfd {
        fd: identity.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll only a live pinned descriptor, with no blocking timeout.
    (unsafe { libc::poll(&mut descriptor, 1, 0) }) == 1 && descriptor.revents & libc::POLLIN != 0
}

struct Sentinel(crate::process_gate::ManagedGate);
impl Sentinel {
    fn start() -> Self {
        let (mut command, route) =
            crate::launch::ProcessLaunch::new("/bin/sleep").native(Some(&runtime().unwrap()));
        command
            .arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut gate =
            crate::process_gate::Admission::new(route.validate_gate(&mut command).unwrap())
                .unwrap()
                .spawn()
                .unwrap();
        gate.resume().unwrap();
        Self(gate)
    }
}
impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = self.0.stop();
    }
}

#[test]
fn blocked_log_writer_cannot_hold_cancellation_or_producer_tree_custody() {
    let root = root();
    let context = context(&root);
    let mut sentinel = Sentinel::start();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let release_on_failure = Release(Arc::clone(&release));
    let reached = Arc::new(AtomicBool::new(false));
    let dropped = Arc::new(AtomicBool::new(false));
    let dependency = Arc::new(Dependency(Arc::clone(&dropped)));
    let lease = ProducerLease::new(dependency, &root);
    let mut step = shell(
        &context,
        "printf $$ > principal.pid; sleep 30 & echo $! > descendant.pid; printf 'inert-private-secret'; wait",
    );
    let held = Arc::clone(&reached);
    let writer_release = Arc::clone(&release);
    step.log_writer = Some(Box::new(move |file| {
        Box::new(HeldWriter {
            file,
            reached: held,
            release: writer_release,
        })
    }));
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        send.send(step.run(lease)).unwrap();
    });
    wait_file(&root.join("principal.pid"));
    wait_file(&root.join("descendant.pid"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !reached.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    // The writer handshake happens after both complete PID writes, avoiding
    // confusing an existing but not-yet-written marker with process identity.
    let principal = pin(fs::read_to_string(root.join("principal.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap());
    let descendant = pin(fs::read_to_string(root.join("descendant.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap());
    assert!(!dropped.load(Ordering::Acquire));
    context.cancel();
    let result = receive
        .recv_timeout(Duration::from_secs(8))
        .expect("cancellation waited for blocked log writer");
    let abort = match result {
        Err(InstallFailure::Abort(abort)) => abort,
        _ => panic!("cancellation was erased"),
    };
    assert!(abort.cleanup_confirmed, "no matching guardian quiescence");
    assert!(exited(&principal));
    assert!(exited(&descendant));
    assert!(
        sentinel.0.try_wait().unwrap().is_none(),
        "unrelated sentinel was stopped"
    );
    assert!(
        !*release.0.lock().unwrap(),
        "writer was released before custody proof"
    );
    // The writer is released only AFTER actual pinned child/descendant exit and
    // the matching native guardian quiescence result were established.
    drop(release_on_failure);
}

#[test]
fn actual_step_deadline_cancels_a_stuck_helper_without_a_reset_budget() {
    let root = root();
    let mut context = context(&root);
    context.deadline = Instant::now() + Duration::from_millis(100);
    let started = Instant::now();
    let failure =
        match shell(&context, "printf started > ready; sleep 30").run(ProducerLease::default()) {
            Err(InstallFailure::Abort(abort)) => abort,
            _ => panic!("deadline became a successful step or an ordinary candidate miss"),
        };
    assert_eq!(failure.cause, AbortCause::Deadline);
    assert!(failure.cleanup_confirmed);
    assert!(started.elapsed() < Duration::from_secs(7));
}

#[test]
fn an_exited_principal_cannot_leave_a_writer_mutating_staging_after_settlement() {
    let root = root();
    let context = context(&root);
    fs::write(root.join("stage"), "initial").unwrap();
    let step = shell(
        &context,
        "(while ! test -f release; do sleep .01; done; printf wrong > stage) & exit 0",
    );
    let result = step.run(ProducerLease::default()).unwrap();
    assert!(result.status().success());
    fs::write(root.join("release"), "release-after-settlement").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fs::read_to_string(root.join("stage")).unwrap(), "initial");
}

struct ObserveRelease(Arc<AtomicBool>);
impl Drop for ObserveRelease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[test]
fn parent_wait_expiry_retains_the_real_dependency_until_matching_quiescence() {
    let root = root();
    let context = context(&root);
    let mut sentinel = Sentinel::start();
    let held = Arc::new(AtomicBool::new(true));
    let release_on_failure = ObserveRelease(Arc::clone(&held));
    let dropped = Arc::new(AtomicBool::new(false));
    let lease = ProducerLease::new(Arc::new(Dependency(Arc::clone(&dropped))), &root);
    let mut step = shell(&context, "printf ready > ready; sleep 30");
    step.hold_observation = Some(Arc::clone(&held));
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = send.send(step.run(lease));
    });
    wait_file(&root.join("ready"));
    context.cancel();
    let failure = match receive.recv_timeout(Duration::from_secs(8)).unwrap() {
        Err(InstallFailure::Abort(abort)) => abort,
        _ => panic!("parent wait was converted into settled success"),
    };
    assert!(!failure.cleanup_confirmed);
    assert!(
        !dropped.load(Ordering::Acquire),
        "unknown cleanup released the actual producer dependency"
    );
    assert!(sentinel.0.try_wait().unwrap().is_none());
    drop(release_on_failure);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dropped.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "matching settlement did not release retained dependency"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(sentinel.0.try_wait().unwrap().is_none());
}

struct ContinueRelease(Arc<AtomicBool>);
impl Drop for ContinueRelease {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn paused_admission_must_not_resume_after_abort(expire: bool) {
    let root = root();
    let mut context = context(&root);
    if expire {
        context.deadline = Instant::now() + Duration::from_millis(500);
    }
    let mut sentinel = Sentinel::start();
    let reached = Arc::new(AtomicBool::new(false));
    let released = Arc::new(AtomicBool::new(false));
    let settled = Arc::new(AtomicBool::new(false));
    let release_guard = ContinueRelease(Arc::clone(&released));
    let mut step = shell(&context, "printf forbidden > client-started; sleep 30");
    step.before_continue = Some(native::AdmissionPause {
        reached: Arc::clone(&reached),
        release: Arc::clone(&released),
        settled: Arc::clone(&settled),
        client_marker: root.join("client-started"),
    });
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = send.send(step.run(ProducerLease::default()));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !reached.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "stopped readiness hook not reached"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    if !expire {
        context.cancel();
    }
    let result = receive.recv_timeout(Duration::from_secs(8)).unwrap();
    assert!(matches!(result, Err(InstallFailure::Abort(_))));
    assert!(!root.join("client-started").exists());
    drop(release_guard);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !settled.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "no actual matching guardian quiescence after release"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(sentinel.0.try_wait().unwrap().is_none());
    assert!(
        !root.join("client-started").exists(),
        "core resumed the client AFTER caller Abort; {}",
        root.display()
    );
}

#[test]
fn core_review_cancelled_stopped_guardian_must_not_resume_after_parent_abort() {
    paused_admission_must_not_resume_after_abort(false);
}
#[test]
fn core_review_expired_stopped_guardian_must_not_resume_after_parent_abort() {
    paused_admission_must_not_resume_after_abort(true);
}

#[test]
fn core_review_step_preparation_uses_frozen_config_not_a_fresh_invalid_file() {
    let root = root();
    fs::create_dir(root.join(".asb")).unwrap();
    let path = root.join(".asb/project.toml");
    let config = ProjectConfig::default();
    fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
    let (_, loaded) = crate::project::load(&root).unwrap();
    let context = OperationContext::new(&root, &loaded).unwrap();
    let mut invalid = config;
    invalid.installation.probe_timeout_seconds = 0;
    fs::write(&path, toml::to_string(&invalid).unwrap()).unwrap();
    assert!(
        crate::project::load(&root).is_err(),
        "replacement fixture was not invalid"
    );
    assert!(
        PreparedStep::new(
            &context,
            "/bin/true",
            Purpose::Probe,
            "frozen config fixture"
        )
        .is_ok(),
        "preparation reloaded a configuration that should be frozen for this operation"
    );
}
