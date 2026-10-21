use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

fn root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/installer-log-core")
        .join(crate::tools::runtime::unique_id());
    fs::create_dir_all(&root).unwrap();
    fs::canonicalize(root).unwrap()
}

struct FaultWriter(Option<i32>);
impl Write for FaultWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        self.0
            .map_or(Ok(0), |code| Err(io::Error::from_raw_os_error(code)))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn ancillary_zero_and_enospc_do_not_claim_retained_bytes_or_complete_logs() {
    for (errno, expected) in [
        (None, io::ErrorKind::WriteZero),
        (Some(libc::ENOSPC), io::ErrorKind::StorageFull),
    ] {
        let root = root();
        let mut sink = LogSink::new(&root, Some(Box::new(move |_| Box::new(FaultWriter(errno)))));
        sink.push(b"inert-private-secret");
        sink.queue.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = sink.report.lock().unwrap().clone();
            if state.settled {
                assert_eq!(state.fault, Some(expected));
                assert_eq!(state.retained, 0);
                assert_eq!(state.discarded, b"inert-private-secret".len());
                assert_eq!(fs::metadata(state.path.unwrap()).unwrap().len(), 0);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn log_namespace_never_overwrites_or_chmods_existing_foreign_entries() {
    let root = root();
    let foreign = root.join(".asb-install-step-foreign");
    fs::write(&foreign, "keep").unwrap();
    fs::set_permissions(&foreign, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(private_log_at(&root, ".asb-install-step-foreign").is_err());
    assert_eq!(fs::read_to_string(&foreign).unwrap(), "keep");
    assert_eq!(
        fs::metadata(&foreign).unwrap().permissions().mode() & 0o777,
        0o644
    );
    symlink(&foreign, root.join(".asb-install-step-link")).unwrap();
    assert!(private_log_at(&root, ".asb-install-step-link").is_err());
    assert!(
        fs::symlink_metadata(root.join(".asb-install-step-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let project_link = root.join("project-link");
    symlink(&root, &project_link).unwrap();
    assert!(
        private_log(&project_link).is_err(),
        "project symlink was followed for raw logging"
    );
    assert_eq!(fs::read_to_string(foreign).unwrap(), "keep");
}

#[test]
fn private_log_modes_hold_under_umask_022_in_an_isolated_owned_process() {
    const CHILD: &str = "ASB_PRIVATE_INSTALL_LOG_TEST_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        // SAFETY: this is an isolated one-test child with its own owned
        // guardian, not the concurrent parent libtest or another process.
        unsafe {
            libc::umask(0o022);
        }
        let (mut file, path) = private_log(Path::new(&root)).unwrap();
        file.write_all(b"inert-private-secret").unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        return;
    }
    let root = root();
    let context = OperationContext::new(&root, &ProjectConfig::default()).unwrap();
    let mut step = PreparedStep::new(
        &context,
        std::env::current_exe().unwrap(),
        Purpose::Probe,
        "private log mode fixture",
    )
    .unwrap();
    step.command_mut().args(["--exact", "install::process::native::tests::private_log_modes_hold_under_umask_022_in_an_isolated_owned_process", "--test-threads=1"])
        .env(CHILD, &root).stdin(Stdio::null());
    assert!(
        step.run(ProducerLease::default())
            .unwrap()
            .status()
            .success()
    );
}
