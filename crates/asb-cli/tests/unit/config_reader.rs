//! Actual configuration input types and the shared serialized size boundary.
use crate::{benchmark::MAX_CONFIG_BYTES, read_config};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/config-reader-fixtures")
            .join(crate::workloads::TaskIdentity::local("config-reader", None).attempt_id);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn actual_configuration_reader_shares_setup_exact_byte_limit_and_names_bad_input() {
    let fixture = Fixture::new();
    let path = fixture.0.join("benchmark.toml");
    fs::write(&path, "x".repeat(MAX_CONFIG_BYTES)).unwrap();
    assert_eq!(read_config(&path).unwrap().len(), MAX_CONFIG_BYTES);
    fs::write(&path, "x".repeat(MAX_CONFIG_BYTES + 1)).unwrap();
    let error = read_config(&path).unwrap_err().to_string();
    assert!(error.contains("larger than 64 KiB"), "{error}");
    assert!(error.contains(&path.display().to_string()), "{error}");
    let error = read_config(&fixture.0).unwrap_err().to_string();
    assert!(error.contains(&fixture.0.display().to_string()), "{error}");
    #[cfg(unix)]
    assert!(error.contains("regular file"), "{error}");
    fs::write(&path, [0xff, 0xfe]).unwrap();
    let error = read_config(&path).unwrap_err().to_string();
    assert!(error.contains("not valid UTF-8"), "{error}");
}

#[cfg(unix)]
#[test]
fn regular_symlink_inputs_remain_supported_and_fifo_is_rejected_without_a_writer() {
    use std::os::unix::{ffi::OsStrExt, fs::symlink};
    let fixture = Fixture::new();
    let path = fixture.0.join("benchmark.toml");
    let alias = fixture.0.join("alias.toml");
    fs::write(&path, "agent='pi'\n").unwrap();
    symlink(&path, &alias).unwrap();
    assert_eq!(read_config(&alias).unwrap(), "agent='pi'\n");
    let fifo = fixture.0.join("fifo.toml");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    #[allow(unsafe_code)]
    // SAFETY: owned fixture pathname with a NUL-terminated representation.
    let result = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
    assert_eq!(result, 0);
    // A future regression removing O_NONBLOCK must fail this fixture, never
    // hang the whole test runner waiting for a FIFO writer.
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .env("ASB_TEST_CONFIG_FIFO", &fifo)
        .args([
            "--exact",
            "config_reader_tests::fifo_reader_subprocess",
            "--nocapture",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let mut child = command.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("configuration reader blocked opening owned FIFO");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(unix)]
#[test]
fn fifo_reader_subprocess() {
    let Some(path) = std::env::var_os("ASB_TEST_CONFIG_FIFO") else {
        return;
    };
    let path = PathBuf::from(path);
    let error = read_config(&path).unwrap_err().to_string();
    assert!(
        error.contains("regular file") && error.contains(&path.display().to_string()),
        "{error}"
    );
}

#[test]
fn actual_opened_reader_bounds_late_growth_and_does_not_reopen_a_replacement() {
    let fixture = Fixture::new();
    let path = fixture.0.join("benchmark.toml");
    fs::write(&path, "opened original").unwrap();
    let error = crate::read_config_after_inspection(&path, || {
        fs::write(&path, vec![b'x'; MAX_CONFIG_BYTES + 1]).unwrap();
    })
    .unwrap_err()
    .to_string();
    assert!(error.contains("larger than 64 KiB"), "{error}");
    fs::write(&path, "opened original").unwrap();
    let result = crate::read_config_after_inspection(&path, || {
        fs::rename(&path, fixture.0.join("retained-original")).unwrap();
        fs::write(&path, "different replacement").unwrap();
    })
    .unwrap();
    assert_eq!(result, "opened original");
    assert_eq!(fs::read_to_string(path).unwrap(), "different replacement");
}
