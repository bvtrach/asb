//! Offline replay-link identity and resume admission. Never runs a benchmark.
use super::*;
use std::fs;
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "asb-sweep-boundaries-{}",
            crate::workloads::TaskIdentity::local("boundary", None).attempt_id
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn configuration() -> BenchmarkConfig {
    toml::from_str("agent='pi'\nprovider='openrouter'\nmodel='fixture/model:1'\nworkload='bug-fix'\nconcurrency=2\n").unwrap()
}

#[test]
fn preserved_replay_links_require_original_identity_not_merely_matching_bytes() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;
    let root = Fixture::new();
    let source = root.0.join("source");
    let destination = root.0.join("destination");
    fs::create_dir(&source).unwrap();
    let config = configuration();
    let filename = cassette_filename(&config);
    assert!(!filename.contains('/'));
    assert!(!filename.contains(':'));
    fs::write(source.join(&filename), "recorded bytes").unwrap();
    fs::write(
        source.join(format!(
            "{}-concurrency-0.jsonl",
            filename.trim_end_matches(".jsonl")
        )),
        "not selected",
    )
    .unwrap();
    fs::write(source.join("unrelated.txt"), "not selected").unwrap();
    let bad_name = std::ffi::OsStr::from_bytes(b"non-utf8-\xff");
    fs::write(source.join(bad_name), "not selected").unwrap();
    reuse_replay_inputs(&source, &destination, &config, false).unwrap();
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);
    assert_eq!(
        fs::metadata(source.join(&filename)).unwrap().ino(),
        fs::metadata(destination.join(&filename)).unwrap().ino()
    );
    reuse_replay_inputs(&source, &destination, &config, true).unwrap();
    fs::remove_file(destination.join(&filename)).unwrap();
    fs::write(destination.join(&filename), "recorded bytes").unwrap();
    assert!(
        reuse_replay_inputs(&source, &destination, &config, true)
            .unwrap_err()
            .to_string()
            .contains("not the preserved recording")
    );
    let blocker = root.0.join("not-directory");
    fs::write(&blocker, "fixture").unwrap();
    assert!(
        reuse_replay_inputs(&blocker, &destination, &config, false)
            .unwrap_err()
            .to_string()
            .contains("cannot inspect preserved replay inputs")
    );
}

#[test]
fn reconciliation_requires_resume_even_before_manifest_is_read() {
    let root = Fixture::new();
    let nonexistent = root.0.join("does-not-exist.toml");
    let options = Options {
        reconcile_completed: true,
        ..Default::default()
    };
    let error = run_with_options(nonexistent.to_str().unwrap(), options, &mut Vec::new())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("--reconcile-completed requires --resume"),
        "{error}"
    );
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
}
