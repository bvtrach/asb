//! Actual private census CLI: bounded, read-only and independent of providers.
#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("asb-material-cli-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn request(&self) -> Value {
        json!({"schema_version":1,"roots":[{"name":"workspace","path":self.0}]})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn invoke(input: &[u8], args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_asb"))
        .arg("__asb-material")
        .args(args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(input);
    child.wait_with_output().unwrap()
}

#[test]
fn private_census_returns_only_bounded_observations_without_files_or_provider_setup() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("input.txt"),
        "fixture-secret-must-not-be-output",
    )
    .unwrap();
    let input = fixture.request().to_string();
    let output = invoke(input.as_bytes(), &[]);
    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["scope"], "declared-root-material-observation-v3");
    assert_eq!(value["schema_version"], 3);
    assert_eq!(value["entries"], 2);
    assert_eq!(value["coverage"]["visible_inode_equivalence"], true);
    assert_eq!(value["coverage"]["complete_namespace"], false);
    assert_eq!(value["coverage"]["extended_attributes"], false);
    assert_eq!(value["coverage"]["visible_extended_attributes"], true);
    assert_eq!(value["coverage"]["atomic_snapshot"], false);
    let encoded = String::from_utf8(output.stdout).unwrap();
    assert!(!encoded.contains("fixture-secret"));
    assert!(!encoded.contains("input.txt"));
    assert!(!encoded.contains(fixture.0.to_str().unwrap()));
    assert_eq!(value["sha256"].as_str().unwrap().len(), 64);
    let again: Value = serde_json::from_slice(&invoke(input.as_bytes(), &[]).stdout).unwrap();
    assert_eq!(value, again);
}

#[test]
fn private_root_witness_authorizes_exact_inode_without_changing_public_identity() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    fs::write(fixture.0.join("input"), b"fixture").unwrap();
    let metadata = fs::symlink_metadata(&fixture.0).unwrap();
    let baseline = invoke(fixture.request().to_string().as_bytes(), &[]);
    assert!(baseline.status.success());
    let mut request = fixture.request();
    request["roots"][0]["expected_identity"] = json!([
        metadata.dev(),
        metadata.ino(),
        metadata.mode() & libc::S_IFMT
    ]);
    let matching = invoke(request.to_string().as_bytes(), &[]);
    assert!(matching.status.success(), "{matching:?}");
    assert_eq!(baseline.stdout, matching.stdout);
    for field in 0..3 {
        let mut wrong = request.clone();
        wrong["roots"][0]["expected_identity"][field] = json!(0);
        let rejected = invoke(wrong.to_string().as_bytes(), &[]);
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&rejected.stderr).contains(fixture.0.to_str().unwrap()));
    }
}

#[test]
fn malformed_oversized_or_unknown_requests_never_echo_input_or_paths() {
    let fixture = Fixture::new();
    let mut unknown = fixture.request();
    unknown["fixture-secret"] = json!("must-not-appear");
    let oversized = vec![b' '; 1024 * 1024 + 2];
    for input in [
        b"not-json".as_slice(),
        unknown.to_string().as_bytes(),
        &oversized,
    ] {
        let output = invoke(input, &[]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("cannot establish bounded declared-root"));
        assert!(!error.contains("fixture-secret"));
        assert!(!error.contains(fixture.0.to_str().unwrap()));
    }
    let output = invoke(fixture.request().to_string().as_bytes(), &["unexpected"]);
    assert!(!output.status.success());
}
