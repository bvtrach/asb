//! Offline public CLI coverage for setup matrices and standalone reports.
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Output};

struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let project = Self(
            std::env::temp_dir().join(format!("asb compare setup {}-{stamp}", std::process::id())),
        );
        project.write(
            ".asb/project.toml",
            "tools_dir='tools'\nworkloads_dir='catalogs'\nworkspace_dir='workspaces'\n",
        );
        project
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn command(&self, args: &[&str]) -> Output {
        self.command_with(args, &[])
    }
    fn command_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_asb"))
            .current_dir(&self.0)
            .env_clear()
            .env("HOME", &self.0)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .envs(env.iter().copied())
            .args(args)
            .output()
            .unwrap()
    }
    fn benchmark(&self, concurrency: usize, measured: bool) {
        let measures = if measured {
            "[measurements]\nenabled=true\ndirectory='captures/profile'\ncollectors=[]\n"
        } else {
            ""
        };
        self.write("configs/benchmark.toml", &format!("agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='bug-fix'\nconcurrency={concurrency}\nlog_file='logs/attempt.log'\n{measures}"));
    }
    fn summary(slot: usize, total: usize, id: &str) -> Value {
        json!({"schema_version":2,"agent":"pi","provider":"openrouter","model":"fixture","workload":"bug-fix",
            "replica_index":slot,"execution_cohort":{"id":"abc123","selected_replicas":total,"admitted_slots":(1..=total).collect::<Vec<_>>()},
            "task":{"workload":"bug-fix","task_id":"bug-fix","attempt_id":id},
            "process_success":true,"workload_success":true,"workload_evaluation":"local-structural",
            "workload_baseline":{"kind":"local-fixture"},"elapsed_ms":123,"steps":4,"token_usage":{"total_tokens":77},
            "status":"completed","stdout":"DO-NOT-PUBLISH-TRANSCRIPT","stderr":"DO-NOT-PUBLISH-TRANSCRIPT"})
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn setup_args() -> Vec<&'static str> {
    vec![
        "setup",
        "--agent",
        "pi",
        "--provider",
        "openrouter",
        "--model",
        "fixture",
        "--workload",
        "bug-fix",
    ]
}

#[test]
fn repeated_selections_generate_scoped_matrix_and_dependencies_without_downloads() {
    let project = Project::new();
    let args = [
        "setup",
        "--agent",
        "pi",
        "--agent",
        "codex",
        "--provider",
        "openrouter",
        "--provider",
        "ollama",
        "--model",
        "openrouter=remote/model",
        "--model",
        "ollama=local-model",
        "--workload",
        "bug-fix",
        "--workload",
        "code-review",
        "--measure",
        "flamegraph",
        "--collector",
        "system",
        "--output",
        "plans/my sweep.toml",
    ];
    success(project.command(&args));
    let manifest = project.0.join("plans/my sweep.toml");
    let sweep: toml::Value = toml::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    let refs = sweep["configs"].as_array().unwrap();
    assert_eq!(refs.len(), 8);
    let mut captures = std::collections::BTreeSet::new();
    for reference in refs {
        let config: toml::Value = toml::from_str(
            &std::fs::read_to_string(manifest.parent().unwrap().join(reference.as_str().unwrap()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            config["model"].as_str().unwrap(),
            if config["provider"].as_str() == Some("ollama") {
                "local-model"
            } else {
                "remote/model"
            }
        );
        let collectors = config["measurements"]["collectors"].as_array().unwrap();
        assert!(collectors.contains(&toml::Value::String("perf".into())));
        assert!(collectors.contains(&toml::Value::String("flamegraph".into())));
        assert!(
            captures.insert(
                config["measurements"]["directory"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            )
        );
    }
    let before = std::fs::read(&manifest).unwrap();
    assert!(!project.command(&args).status.success());
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
    // All generated references also work through the ordinary sweep reader.
    success(project.command(&["compare", "plans/my sweep.toml"]));
    assert!(
        project
            .0
            .join("plans/results/my sweep-comparison/report.html")
            .is_file()
    );
}

#[test]
fn every_repeatable_selection_triggers_sweep_even_for_duplicate_values() {
    for (flag, value) in [
        ("--agent", "pi"),
        ("--provider", "openrouter"),
        ("--model", "fixture"),
        ("--workload", "bug-fix"),
        ("--measure", "system"),
    ] {
        let project = Project::new();
        let mut args = setup_args();
        if flag == "--measure" {
            args.extend([flag, value]);
        }
        args.extend([flag, value, "--output", "matrix.toml"]);
        success(project.command(&args));
        let sweep: toml::Value =
            toml::from_str(&std::fs::read_to_string(project.0.join("matrix.toml")).unwrap())
                .unwrap();
        assert_eq!(sweep["configs"].as_array().unwrap().len(), 1, "{flag}");
    }
}

#[test]
fn trial_sets_expand_enabled_trials_and_explicit_scopes_select_subsets() {
    let project = Project::new();
    project.write(
        "catalogs/agentbench.workload.toml",
        "workload='agentbench'\n[[trials]]\nid='one'\n[[trials]]\nid='two'\n",
    );
    project.write(
        "catalogs/gaia.workload.toml",
        "workload='gaia'\n[[trials]]\nid='three'\n",
    );
    let args = [
        "setup",
        "--agent",
        "pi",
        "--provider",
        "openrouter",
        "--model",
        "fixture",
        "--workload",
        "agentbench",
        "--workload",
        "gaia",
        "--trial-set",
        "agentbench.workload.toml",
        "--trial-set",
        "gaia.workload.toml",
        "--output",
        "trials.toml",
    ];
    success(project.command(&args));
    let sweep: toml::Value =
        toml::from_str(&std::fs::read_to_string(project.0.join("trials.toml")).unwrap()).unwrap();
    assert_eq!(sweep["configs"].as_array().unwrap().len(), 3);
    let mut subset = args.to_vec();
    *subset.last_mut().unwrap() = "subset.toml";
    subset.extend(["--trial", "agentbench=two", "--trial", "gaia=three"]);
    success(project.command(&subset));
    let sweep: toml::Value =
        toml::from_str(&std::fs::read_to_string(project.0.join("subset.toml")).unwrap()).unwrap();
    assert_eq!(sweep["configs"].as_array().unwrap().len(), 2);
}

#[test]
fn invalid_matrix_does_not_publish_partial_files() {
    for extra in [
        vec!["--model", "unselected=model"],
        vec!["--measure", "not-a-collector"],
        vec!["--agent", "opencod"],
        vec!["--workload", ""],
    ] {
        let project = Project::new();
        let mut args = setup_args();
        args.extend(extra);
        args.extend(["--output", "not-created/sweep.toml"]);
        assert!(!project.command(&args).status.success());
        assert!(!project.0.join("not-created").exists());
    }
}

#[test]
fn single_report_uses_custom_measurements_and_latest_log_without_transcripts() {
    let project = Project::new();
    project.benchmark(1, true);
    let old = Project::summary(1, 1, "previous");
    let current = Project::summary(1, 1, "current");
    project.write("configs/logs/attempt.log", &format!("{old}\n{current}\n"));
    project.write("captures/profile/run.json", &current.to_string());
    project.write("captures/profile/lifecycle.json", "[]");
    success(project.command(&["compare", "configs/benchmark.toml"]));
    let report: Value = serde_json::from_slice(
        &std::fs::read(
            project
                .0
                .join("configs/results/benchmark-comparison/comparison.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["benchmarks"][0]["task"]["attempt_id"], "current");
    assert_eq!(report["benchmarks"][0]["result_source"], "measurements");
    assert_eq!(
        report["benchmarks"][0]["metrics"]["token_usage.total_tokens"],
        77.0
    );
    assert_eq!(report["input_kind"], "benchmark");
    for artifact in report["benchmarks"][0]["artifacts"].as_array().unwrap() {
        assert!(
            project
                .0
                .join("configs/results/benchmark-comparison")
                .join(artifact.as_str().unwrap())
                .is_file()
        );
    }
    assert!(
        report["benchmarks"][0]["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact.as_str().unwrap().contains("lifecycle.json"))
    );
    let html = std::fs::read_to_string(
        project
            .0
            .join("configs/results/benchmark-comparison/report.html"),
    )
    .unwrap();
    assert!(html.contains("metric-elapsed_ms.svg"));
    assert!(!html.contains("DO-NOT-PUBLISH-TRANSCRIPT"));
    let mut stale = old;
    stale["status"] = json!("completed");
    project.write("captures/profile/run.json", &stale.to_string());
    assert!(
        !project
            .command(&["compare", "configs/benchmark.toml"])
            .status
            .success()
    );
}

#[test]
fn unmeasured_concurrent_single_report_includes_each_replica() {
    let project = Project::new();
    project.benchmark(2, false);
    for index in 1..=2 {
        let mut value = Project::summary(index, 2, &format!("attempt-{index}"));
        if index == 2 {
            value["process_success"] = json!(false);
            value["failure_reason"] = json!("fixture agent failed");
        }
        project.write(
            &format!("configs/logs/attempt-concurrency-{index}.log"),
            &format!("{value}\n"),
        );
    }
    success(project.command(&["compare", "configs/benchmark.toml"]));
    let report: Value = serde_json::from_slice(
        &std::fs::read(
            project
                .0
                .join("configs/results/benchmark-comparison/comparison.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["benchmarks"].as_array().unwrap().len(), 2);
    assert_eq!(report["benchmarks"][0]["result_source"], "run-log");
    assert!(
        report["benchmarks"][1]["status"]
            .as_str()
            .unwrap()
            .contains("fixture agent failed")
    );
    assert!(
        project
            .0
            .join("configs/results/benchmark-comparison/metric-elapsed_ms.svg")
            .is_file()
    );
}

#[test]
fn single_report_rejects_changed_identity_and_partial_latest_records() {
    let project = Project::new();
    project.benchmark(1, false);
    let original = Project::summary(1, 1, "attempt");
    for field in ["agent", "provider", "model", "workload"] {
        let mut changed = original.clone();
        changed[field] = json!("different-selection");
        project.write("configs/logs/attempt.log", &format!("{changed}\n"));
        let result = project.command(&["compare", "configs/benchmark.toml"]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("does not match selected"));
    }
    project.write(
        "configs/logs/attempt.log",
        &format!("{original}\n{{\"unfinished\":"),
    );
    let result = project.command(&["compare", "configs/benchmark.toml"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid latest run log JSON"));
    // A large history must not force reading the full log or select its first record.
    let mut history = " \n".repeat(8 * 1024 * 1024 + 1);
    history.push_str(&format!("{original}\n"));
    project.write("configs/logs/attempt.log", &history);
    success(project.command(&["compare", "configs/benchmark.toml"]));
}

#[test]
fn repeated_trial_and_trial_set_also_generate_sweeps() {
    for extra in [
        vec!["--trial", "one", "--trial", "one"],
        vec!["--trial-set", "selected.toml"],
    ] {
        let project = Project::new();
        project.write(
            "catalogs/selected.toml",
            "workload='agentbench'\n[[trials]]\nid='one'\n",
        );
        let mut args = vec![
            "setup",
            "--agent",
            "pi",
            "--provider",
            "openrouter",
            "--model",
            "fixture",
            "--workload",
            "agentbench",
            "--trial-set",
            "selected.toml",
            "--output",
            "matrix.toml",
        ];
        args.extend(extra);
        success(project.command(&args));
        let sweep: toml::Value =
            toml::from_str(&std::fs::read_to_string(project.0.join("matrix.toml")).unwrap())
                .unwrap();
        assert_eq!(sweep["configs"].as_array().unwrap().len(), 1);
    }
}

fn configurations(project: &Project, manifest: &str) -> Vec<toml::Value> {
    let path = project.0.join(manifest);
    let config: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    if let Some(references) = config.get("configs") {
        references
            .as_array()
            .unwrap()
            .iter()
            .map(|reference| {
                toml::from_str(
                    &std::fs::read_to_string(
                        path.parent().unwrap().join(reference.as_str().unwrap()),
                    )
                    .unwrap(),
                )
                .unwrap()
            })
            .collect()
    } else {
        vec![config]
    }
}

#[test]
fn all_agents_providers_and_measures_generate_a_matrix_without_literal_all() {
    let project = Project::new();
    success(project.command(&[
        "setup",
        "--agent",
        "all",
        "--provider",
        "all",
        "--model",
        "fixture",
        "--workload",
        "bug-fix",
        "--measure",
        "all",
        "--output",
        "all-selections.toml",
    ]));
    let configs = configurations(&project, "all-selections.toml");
    let agents = configs
        .iter()
        .map(|config| config["agent"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let providers = configs
        .iter()
        .map(|config| config["provider"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(agents.contains("codex") && agents.contains("opencode") && agents.contains("pi"));
    assert!(providers.contains("openrouter") && providers.contains("ollama"));
    assert_eq!(configs.len(), agents.len() * providers.len());
    for config in configs {
        for field in ["agent", "provider", "model", "workload"] {
            assert_ne!(config[field].as_str(), Some("all"));
        }
        let measures = config["measurements"]["collectors"].as_array().unwrap();
        assert!(measures.len() > 3);
        assert!(!measures.iter().any(|value| value.as_str() == Some("all")));
    }
    assert!(
        !project.0.join("tools").exists(),
        "selection must not install tools"
    );
}

#[test]
fn all_trial_sets_and_scoped_all_trials_match_explicit_selections() {
    let project = Project::new();
    project.write(
        "catalogs/a.workload.toml",
        "workload='agentbench'\n[[trials]]\nid='one'\n[[trials]]\nid='two'\n",
    );
    project.write(
        "catalogs/b.workload.toml",
        "workload='gaia'\n[[trials]]\nid='three'\n",
    );
    let mut args = vec![
        "setup",
        "--agent",
        "pi",
        "--provider",
        "openrouter",
        "--model",
        "fixture",
        "--workload",
        "agentbench",
        "--workload",
        "gaia",
        "--trial-set",
        "all",
        "--trial",
        "agentbench=all",
        "--trial",
        "gaia=all",
        "--output",
        "all-trials.toml",
    ];
    success(project.command(&args));
    let actual = configurations(&project, "all-trials.toml");
    args = vec![
        "setup",
        "--agent",
        "pi",
        "--provider",
        "openrouter",
        "--model",
        "fixture",
        "--workload",
        "agentbench",
        "--workload",
        "gaia",
        "--trial-set",
        "a.workload.toml",
        "--trial-set",
        "b.workload.toml",
        "--trial",
        "agentbench=one",
        "--trial",
        "agentbench=two",
        "--trial",
        "gaia=three",
        "--output",
        "explicit-trials.toml",
    ];
    success(project.command(&args));
    let expected = configurations(&project, "explicit-trials.toml");
    let identities = |configs: Vec<toml::Value>| {
        configs
            .into_iter()
            .map(|config| {
                (
                    config["workload"].as_str().unwrap().to_owned(),
                    config["trial"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(identities(actual), identities(expected));
}

#[test]
fn reserved_dynamic_workload_trial_and_trial_set_names_are_rejected_before_publication() {
    for (filename, body, flag) in [
        (
            "custom.workload.toml",
            "workload='all'\n[[trials]]\nid='one'\n",
            "--trial-set",
        ),
        (
            "custom.workload.toml",
            "workload='agentbench'\n[[trials]]\nid='all'\n",
            "--trial-set",
        ),
        (
            "all.workload.toml",
            "workload='agentbench'\n[[trials]]\nid='one'\n",
            "--trial-set",
        ),
        ("all.txt", "fixture prompt", "--workload"),
    ] {
        let project = Project::new();
        project.write(&format!("catalogs/{filename}"), body);
        let result = project.command(&[
            "setup",
            "--agent",
            "pi",
            "--provider",
            "openrouter",
            "--model",
            "fixture",
            "--workload",
            "agentbench",
            flag,
            "all",
            "--output",
            "not-created/sweep.toml",
        ]);
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("reserved"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!project.0.join("not-created").exists());
    }
}

#[test]
fn all_models_discovers_once_on_loopback_and_preserves_provider_scoping() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    for (models, succeeds, scoped) in [
        (vec!["first", "second"], true, true),
        (vec!["first", "second"], true, false),
        (vec!["all"], false, true),
        (vec![], false, false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let body = json!({"data":models.iter().map(|id| json!({"id":id,"supported_parameters":["tools"]})).collect::<Vec<_>>()}).to_string();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    result => panic!("fixture catalog request did not arrive: {result:?}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            assert!(String::from_utf8_lossy(&request).starts_with("GET /v1/models "));
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let project = Project::new();
        let mut args = vec![
            "setup",
            "--agent",
            "pi",
            "--provider",
            "openrouter",
            "--workload",
            "bug-fix",
            "--output",
            "models.toml",
        ];
        if scoped {
            args.extend([
                "--provider",
                "ollama",
                "--model",
                "openrouter=all",
                "--model",
                "openrouter=all",
                "--model",
                "ollama=local-fixture",
            ]);
        } else {
            args.extend(["--model", "all"]);
        }
        let result = project.command_with(&args, &[("OPENROUTER_BASE_URL", &base)]);
        server.join().unwrap();
        if succeeds {
            success(result);
            let configs = configurations(&project, "models.toml");
            assert_eq!(configs.len(), if scoped { 3 } else { 2 });
            for config in configs {
                if config["provider"].as_str() == Some("ollama") {
                    assert_eq!(config["model"].as_str(), Some("local-fixture"));
                } else {
                    assert!(matches!(config["model"].as_str(), Some("first" | "second")));
                }
            }
        } else {
            assert!(!result.status.success());
            assert!(
                String::from_utf8_lossy(&result.stderr).contains(if models.is_empty() {
                    "no tool-capable models"
                } else {
                    "reserved"
                })
            );
            assert!(!project.0.join("models.toml").exists());
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn report_flag_opens_generated_html_once_and_reports_opener_failures() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new();
    // No log or measurements: a truthful missing-result report is still valid.
    project.benchmark(1, false);
    project.write(
        "bin/xdg-open",
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$HOME/opened\"\nexit 0\n",
    );
    std::fs::set_permissions(
        project.0.join("bin/xdg-open"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let path = project.0.join("bin").to_string_lossy().into_owned();
    success(project.command_with(&["compare", "configs/benchmark.toml"], &[("PATH", &path)]));
    assert!(!project.0.join("opened").exists());
    success(project.command_with(
        &["compare", "--report", "configs/benchmark.toml"],
        &[("PATH", &path)],
    ));
    let opened = std::fs::read_to_string(project.0.join("opened")).unwrap();
    assert_eq!(opened.lines().count(), 1);
    assert_eq!(
        PathBuf::from(opened.trim()),
        project
            .0
            .join("configs/results/benchmark-comparison/report.html")
    );
    assert!(
        !project
            .command_with(&["compare", "--report", "missing.toml"], &[("PATH", &path)])
            .status
            .success()
    );
    assert_eq!(
        std::fs::read_to_string(project.0.join("opened")).unwrap(),
        opened
    );
    project.write("bin/xdg-open", "#!/bin/sh\nexit 7\n");
    let result = project.command_with(
        &["compare", "--report", "configs/benchmark.toml"],
        &[("PATH", &path)],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("report created at"));
}
