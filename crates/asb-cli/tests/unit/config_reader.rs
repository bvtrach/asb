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
    let error = read_config(&fifo).unwrap_err().to_string();
    assert!(error.contains("regular file"), "{error}");
    assert!(error.contains(&fifo.display().to_string()), "{error}");
}
