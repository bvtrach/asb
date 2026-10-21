//! Hermetic lifecycle failures and local protocol fixtures, no live upstream.
use super::*;
use std::fs;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "asb-tape-boundaries-{}",
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

#[test]
fn startup_errors_are_specific_and_remove_only_their_private_scratch() {
    const CHILD: &str = "ASB_TEST_TAPE_BOUNDARY";
    if let Ok(scenario) = std::env::var(CHILD) {
        let root = Fixture::new();
        let mut settings = Settings {
            mode: Mode::Record,
            cassette: root.0.join("tape.jsonl"),
            respect_record_timings: false,
            workspace: root.0.join("workspace"),
        };
        let error = match scenario.as_str() {
            "missing-upstream" => ADAPTER.start(None, Some(&settings)).err().unwrap(),
            "missing-cassette" => {
                settings.mode = Mode::Replay;
                ADAPTER.start(None, Some(&settings)).err().unwrap()
            }
            "blocked-parent" => {
                fs::write(root.0.join("blocked"), "file").unwrap();
                settings.cassette = root.0.join("blocked/tape.jsonl");
                ADAPTER.start(None, Some(&settings)).err().unwrap()
            }
            "missing-node" => ADAPTER
                .start(Some("http://127.0.0.1:1/v1"), Some(&settings))
                .err()
                .unwrap(),
            "override" => {
                assert!(matches!(
                    resolve_executable(&root.0).unwrap(),
                    ProxyExecutable::Override(_)
                ));
                ADAPTER
                    .start(Some("http://127.0.0.1:1/v1"), Some(&settings))
                    .err()
                    .unwrap()
            }
            _ => panic!("unknown private fixture"),
        }
        .to_string();
        let expected = match scenario.as_str() {
            "missing-upstream" => "record requires a provider API endpoint",
            "missing-cassette" => "cannot copy replay cassette",
            "blocked-parent" => "cannot create cassette directory",
            "missing-node" => "Install Node.js",
            "override" => "Check the ASB_LLM_TAPE executable",
            _ => unreachable!(),
        };
        assert!(error.contains(expected), "{scenario}: {error}");
        assert!(
            !fs::read_dir(&root.0).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".asb-tape-")),
            "failed startup left private state"
        );
        return;
    }
    for scenario in [
        "missing-upstream",
        "missing-cassette",
        "blocked-parent",
        "missing-node",
        "override",
    ] {
        let root = Fixture::new();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact","proxy::adapters::llmtape::boundary_tests::startup_errors_are_specific_and_remove_only_their_private_scratch","--nocapture"]).env_clear().env(CHILD,scenario).env("HOME",&root.0).env("PATH",&root.0).current_dir(&root.0);
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            child.env("LLVM_PROFILE_FILE", profile);
        }
        if scenario == "override" {
            child.env("ASB_LLM_TAPE", root.0.join("absent-override"));
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn failed_or_nonfile_readiness_is_not_treated_as_pending() {
    let root = Fixture::new();
    fs::create_dir(root.0.join("directory")).unwrap();
    assert!(
        read_ready_port(&root.0.join("directory"), 1)
            .unwrap_err()
            .to_string()
            .contains("not a regular file")
    );
    std::os::unix::fs::symlink("loop", root.0.join("loop")).unwrap();
    assert!(
        read_ready_port(&root.0.join("loop"), 1)
            .unwrap_err()
            .to_string()
            .contains("cannot read tape readiness")
    );
}

#[test]
fn early_proxy_exit_and_legacy_listener_have_distinct_readiness_results() {
    let root = Fixture::new();
    let mut dead = Command::new("/bin/true").spawn().unwrap();
    dead.wait().unwrap();
    let mut dead = StartingChild(Some(dead));
    assert!(
        wait_for_ready(
            &mut dead,
            Some(&root.0.join("absent")),
            0,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .to_string()
        .contains("exited before becoming ready")
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let child = Command::new("/bin/sleep").arg("10").spawn().unwrap();
    let mut child = StartingChild(Some(child));
    assert_eq!(
        wait_for_ready(&mut child, None, port, Duration::from_secs(1)).unwrap(),
        port
    );
    drop(child); // Only this fixture's child is killed and reaped.
}

#[test]
fn replay_finish_reports_absent_and_malformed_progress_then_is_idempotent() {
    let root = Fixture::new();
    for (progress, contents, expected) in [
        (None, None, "progress path is missing"),
        (Some("absent"), None, "did not produce cassette progress"),
        (Some("invalid"), Some("{"), "progress is malformed"),
    ] {
        let mut child = Command::new("/bin/true").spawn().unwrap();
        child.wait().unwrap();
        let progress = progress.map(|name| root.0.join(name));
        if let (Some(path), Some(contents)) = (&progress, contents) {
            fs::write(path, contents).unwrap();
        }
        let mut session = RunningLlmtape {
            child: Some(child),
            builtin: false,
            url: "http://127.0.0.1:1/v1".into(),
            progress,
            cassette: root.0.join("tape.jsonl"),
            mode: Mode::Replay,
            cleanup_confirmed: false,
            _scratch: Scratch::create(&root.0).unwrap(),
        };
        assert!(session.finish().unwrap_err().to_string().contains(expected));
        assert!(session.cleanup_confirmed());
        assert!(session.finish().is_ok());
    }
}

#[test]
fn materializing_builtin_script_failure_does_not_silently_use_an_incomplete_proxy() {
    // Explicitly isolate environment so a developer override cannot hide this path.
    const CHILD: &str = "ASB_TEST_TAPE_MATERIALIZE";
    if std::env::var_os(CHILD).is_some() {
        let root = Fixture::new();
        fs::create_dir(root.0.join(".asb-tape-proxy.mjs")).unwrap();
        let error = resolve_executable(&root.0).err().unwrap().to_string();
        assert!(error.contains("cannot materialize ASB's built-in tape proxy"));
        return;
    }
    let root = Fixture::new();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact","proxy::adapters::llmtape::boundary_tests::materializing_builtin_script_failure_does_not_silently_use_an_incomplete_proxy"]).env_clear().env(CHILD,"1").env("HOME",&root.0).current_dir(&root.0);
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
