//! Real CLI repeated sweeps, sharing the owned synthetic run fixtures.
use super::*;

fn sweep(project: &Project, provider: Option<&RecordingProvider>, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asb"));
    command
        .current_dir(&project.0)
        .isolated_env()
        .env("HOME", &project.0)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args(["sweep", "sweep.toml", "--json"])
        .args(arguments);
    if let Some(provider) = provider {
        command
            .env("OPENROUTER_API_KEY", "fixture-only-key")
            .env("OPENROUTER_BASE_URL", format!("{}/v1", provider.endpoint))
            .env(
                "OPENROUTER_MODELS_URL",
                format!("{}/models", provider.endpoint),
            );
    }
    command.output().unwrap()
}

fn selections(output: &Output) -> std::collections::BTreeMap<String, Value> {
    records(&output.stdout)
        .into_iter()
        .filter(|row| row["type"] == "replay-selection")
        .map(|row| {
            (
                row["selection"]["manifest_sha256"].as_str().unwrap().into(),
                row["selection"].clone(),
            )
        })
        .collect()
}

fn legacy_name(project: &Project) -> String {
    // This fixture's absolute path contains only the legacy slash separator
    // outside the allowed label alphabet. Production naming remains shared.
    let component = project
        .0
        .join("fixture-agent")
        .to_str()
        .unwrap()
        .replace('/', "-");
    format!("benchmark-{component}-bug-fix-fixture")
}

#[test]
fn repeated_sweep_records_one_bundle_per_configuration_and_replays_identically_across_jobs() {
    let project = Project::new();
    project.config(2);
    let config = fs::read_to_string(project.0.join("benchmark.toml"))
        .unwrap()
        .replace("provider='fixture'", "provider='openrouter'");
    for name in ["a.toml", "b.toml"] {
        fs::write(project.0.join(name), &config).unwrap();
    }
    fs::write(
        project.0.join("sweep.toml"),
        "configs=['a.toml','b.toml']\n",
    )
    .unwrap();
    let provider = RecordingProvider::new();
    let recorded = sweep(
        &project,
        Some(&provider),
        &[
            "--record",
            "--repeat",
            "2",
            "--cassette-root",
            "recordings",
            "--jobs",
            "2",
            "--max-active-attempts",
            "4",
        ],
    );
    assert!(
        recorded.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&recorded.stdout),
        String::from_utf8_lossy(&recorded.stderr)
    );
    let index_path = project.0.join("sweep-results/plan.json");
    let index: Value = serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
    assert_eq!(index["schema_version"], 2);
    assert_eq!(index["execution"]["repeat"], 2);
    assert_eq!(index["execution"]["recording_reservation"]["references"], 8);
    let mut reserved = asb_cli::cassette::RecordingReservation::default();
    for (number, name) in ["a", "b"].into_iter().enumerate() {
        assert!(
            index["entries"][number]["campaign"]["root"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{number:03}-{name}/campaigns/"))
        );
        let path = project.0.join(format!(
            "recordings/{number:03}-{name}/cassettes/recording.asb.zip"
        ));
        let archive =
            asb_cli::cassette::Archive::open(&path, asb_cli::cassette::Limits::default()).unwrap();
        assert_eq!(archive.manifest().tracks.len(), 4);
        let campaign = project.0.join("sweep-results").join(
            index["entries"][number]["campaign"]["root"]
                .as_str()
                .unwrap(),
        );
        let retained: Value =
            serde_json::from_slice(&fs::read(campaign.join("plan.json")).unwrap()).unwrap();
        assert_eq!(retained["recording_selection"]["task_id"], "bug-fix");
        let capacity: asb_cli::cassette::RecordingReservation =
            serde_json::from_value(retained["recording_reservation"].clone()).unwrap();
        assert_eq!(capacity.references, 4);
        reserved = reserved.checked_add(&capacity).unwrap();
    }
    assert!(
        project
            .0
            .join("sweep-results/campaigns-published.json")
            .is_file()
    );
    let report_path = project.0.join("results/sweep-comparison/comparison.json");
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["benchmarks"].as_array().unwrap().len(), 8);
    assert_eq!(report["selection_plan"]["execution_mode"], "record");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 8);
    assert_eq!(
        serde_json::to_value(&reserved).unwrap(),
        index["execution"]["recording_reservation"]
    );
    let report_bytes = fs::read(&report_path).unwrap();
    let index_bytes = fs::read(&index_path).unwrap();
    for field in [
        "references",
        "reference_bytes",
        "journal_bytes",
        "manifest_bytes",
        "publication_bytes",
        "directory_bytes",
        "schema_version",
    ] {
        let mut changed = index.clone();
        let value = changed["execution"]["recording_reservation"][field]
            .as_u64()
            .unwrap();
        changed["execution"]["recording_reservation"][field] = json!(value + 1);
        fs::write(&index_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        let refused = Command::new(env!("CARGO_BIN_EXE_asb"))
            .current_dir(&project.0)
            .isolated_env()
            .args(["compare", "sweep.toml"])
            .output()
            .unwrap();
        assert!(!refused.status.success(), "accepted changed {field}");
        assert_eq!(fs::read(&report_path).unwrap(), report_bytes);
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 8);
    }
    fs::write(&index_path, index_bytes).unwrap();
    let mut previous = None;
    for jobs in ["1", "2"] {
        let replayed = sweep(
            &project,
            None,
            &[
                "--replay",
                "--repeat",
                "3",
                "--seed",
                "73",
                "--cassette-root",
                "recordings",
                "--jobs",
                jobs,
                "--max-active-attempts",
                "4",
            ],
        );
        assert!(
            replayed.status.success(),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&replayed.stdout),
            String::from_utf8_lossy(&replayed.stderr)
        );
        let choices = selections(&replayed);
        assert_eq!(choices.len(), 2);
        for selection in choices.values() {
            assert_eq!(selection["seed"], 73);
            assert_eq!(selection["tracks"].as_array().unwrap().len(), 6);
        }
        if let Some(previous) = &previous {
            assert_eq!(previous, &choices);
        }
        previous = Some(choices);
        let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        let rows = report["benchmarks"].as_array().unwrap();
        assert_eq!(rows.len(), 12);
        let mut identities = std::collections::BTreeSet::new();
        for row in rows {
            assert!(identities.insert(row["id"].to_string()));
            assert_eq!(row["execution"]["replay_seed"], 73);
            assert_eq!(row["execution_cohort"]["selected_replicas"], 2);
        }
        assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 8);
    }
    assert_eq!(
        records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
        32
    );
    let old_index = fs::read(&index_path).unwrap();
    let old_report = fs::read(&report_path).unwrap();
    fs::remove_file(
        project
            .0
            .join("recordings/001-b/cassettes/recording.asb.zip"),
    )
    .unwrap();
    let refused = sweep(
        &project,
        None,
        &["--replay", "--repeat", "2", "--cassette-root", "recordings"],
    );
    assert!(!refused.status.success());
    assert_eq!(fs::read(index_path).unwrap(), old_index);
    assert_eq!(fs::read(report_path).unwrap(), old_report);
    assert_eq!(
        records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
        32
    );
}

#[test]
fn recording_sweep_global_metadata_preflight_preserves_old_results_without_clients() {
    let project = Project::new();
    project.config(1);
    let config = fs::read_to_string(project.0.join("benchmark.toml"))
        .unwrap()
        .replace("provider='fixture'", "provider='openrouter'")
        .replace("model='fixture'", &format!("model='{}'", "m".repeat(4096)));
    let mut names = Vec::new();
    for index in 0..60 {
        let name = format!("config-{index:02}.toml");
        fs::write(project.0.join(&name), &config).unwrap();
        names.push(name);
    }
    fs::write(
        project.0.join("sweep.toml"),
        format!("configs={}\n", serde_json::to_string(&names).unwrap()),
    )
    .unwrap();
    let index = project.0.join("sweep-results/plan.json");
    let report = project.0.join("results/sweep-comparison/comparison.json");
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    fs::create_dir_all(report.parent().unwrap()).unwrap();
    fs::write(&index, b"preserved original index").unwrap();
    fs::write(&report, b"preserved original report").unwrap();
    let provider = RecordingProvider::new();
    let refused = sweep(&project, Some(&provider), &["--record", "--repeat", "1000"]);
    assert!(!refused.status.success());
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(message.contains("aggregate metadata budget"), "{message}");
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(!project.0.join("attempts.jsonl").exists());
    assert!(!project.0.join("barrier-1-1").exists());
    assert!(!project.0.join("cassettes").exists());
    assert!(!project.0.join("sweep-history").exists());
    assert_eq!(fs::read(index).unwrap(), b"preserved original index");
    assert_eq!(fs::read(report).unwrap(), b"preserved original report");
}

#[test]
fn default_sweep_bundle_survives_history_and_replay_concurrency_changes() {
    let project = Project::new();
    project.config(1);
    let config_path = project.0.join("benchmark.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("provider='fixture'", "provider='openrouter'");
    let config = format!(
        "{config}\n[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n"
    );
    fs::write(&config_path, &config).unwrap();
    fs::write(project.0.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let provider = RecordingProvider::new();
    let recorded = sweep(&project, Some(&provider), &["--record"]);
    assert!(
        recorded.status.success(),
        "{}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    let original_plan = campaign_plan(&recorded);
    let original: Value = serde_json::from_slice(&fs::read(&original_plan).unwrap()).unwrap();
    let recording = PathBuf::from(original["recording"].as_str().unwrap());
    assert!(recording.starts_with(project.0.join("cassettes")));
    fs::write(
        &config_path,
        config.replace("concurrency=1", "concurrency=2"),
    )
    .unwrap();
    fs::write(project.0.join("expected-concurrency"), "2").unwrap();
    let replayed = sweep(
        &project,
        None,
        &["--replay", "--repeat", "2", "--seed", "18"],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let replay_index: Value =
        serde_json::from_slice(&fs::read(project.0.join("sweep-results/plan.json")).unwrap())
            .unwrap();
    let preserved = project
        .0
        .join("sweep-history")
        .join(replay_index["run_id"].as_str().unwrap())
        .join("sweep-results")
        .join(
            original_plan
                .strip_prefix(project.0.join("sweep-results"))
                .unwrap(),
        );
    let historical = compare_campaign(&project, &preserved);
    assert!(
        historical.status.success(),
        "{}",
        String::from_utf8_lossy(&historical.stderr)
    );
    let historical_report: Value = serde_json::from_slice(
        &fs::read(
            preserved
                .parent()
                .unwrap()
                .join("comparison/comparison.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        historical_report["benchmarks"][0]["result_source"],
        "measurements"
    );
    assert_eq!(
        historical_report["benchmarks"][0]["execution_cohort"]["selected_replicas"],
        1
    );
    let again = sweep(&project, None, &["--replay"]);
    assert!(
        again.status.success(),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert!(recording.is_file());
    assert_eq!(provider.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
        7
    );
    let index = project.0.join("sweep-results/plan.json");
    let before = fs::read(&index).unwrap();
    for arguments in [
        vec!["--import-grades"],
        vec!["--replay", "--resume"],
        vec!["--replay", "--resume", "--repeat", "1"],
        vec!["--replay", "--resume", "--reconcile-completed"],
    ] {
        assert!(!sweep(&project, None, &arguments).status.success());
        assert_eq!(fs::read(&index).unwrap(), before);
        assert_eq!(
            records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
            7
        );
    }
}

#[test]
fn numeric_legacy_sweep_intent_cannot_relabel_frozen_track_choices() {
    let project = Project::new();
    project.config(1);
    fs::write(project.0.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let directory = project.0.join("legacy/000-benchmark/cassettes");
    fs::create_dir_all(&directory).unwrap();
    // Dispatch is content-based, including a legacy JSONL with a ZIP suffix.
    fs::copy(
        project.0.join("input.jsonl"),
        directory.join("recording.asb.zip"),
    )
    .unwrap();
    let replayed = sweep(
        &project,
        None,
        &[
            "--replay",
            "--repeat",
            "2",
            "--track",
            "1",
            "--cassette-root",
            "legacy",
        ],
    );
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    let index_path = project.0.join("sweep-results/plan.json");
    let original = fs::read(&index_path).unwrap();
    let report_path = project.0.join("results/sweep-comparison/comparison.json");
    let report = fs::read(&report_path).unwrap();
    for selector in [json!({"id":2}), json!({"min":"tokens"}), Value::Null] {
        let mut index: Value = serde_json::from_slice(&original).unwrap();
        index["execution"]["selector"] = selector;
        fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        let compared = Command::new(env!("CARGO_BIN_EXE_asb"))
            .current_dir(&project.0)
            .isolated_env()
            .args(["compare", "sweep.toml"])
            .output()
            .unwrap();
        assert!(!compared.status.success());
        assert_eq!(fs::read(&report_path).unwrap(), report);
    }
    fs::write(index_path, original).unwrap();
    assert_eq!(
        records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
        2
    );
}

#[test]
fn invalid_present_sweep_bundle_never_falls_back_to_valid_legacy_input() {
    let project = Project::new();
    project.config(1);
    fs::write(project.0.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let directory = project.0.join("legacy/000-benchmark/cassettes");
    fs::create_dir_all(&directory).unwrap();
    let legacy = directory.join(format!("{}.jsonl", legacy_name(&project)));
    fs::copy(project.0.join("input.jsonl"), &legacy).unwrap();
    let arguments = ["--replay", "--repeat", "2", "--cassette-root", "legacy"];
    let good = sweep(&project, None, &arguments);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    let index = project.0.join("sweep-results/plan.json");
    let report = project.0.join("results/sweep-comparison/comparison.json");
    let previous_index = fs::read(&index).unwrap();
    let previous_report = fs::read(&report).unwrap();
    let bundle = directory.join("recording.asb.zip");
    for kind in ["dangling", "directory"] {
        if kind == "dangling" {
            std::os::unix::fs::symlink(directory.join("absent"), &bundle).unwrap();
        } else {
            fs::create_dir(&bundle).unwrap();
        }
        let refused = sweep(&project, None, &arguments);
        assert!(
            !refused.status.success(),
            "{kind} silently selected legacy tape"
        );
        assert_eq!(fs::read(&index).unwrap(), previous_index);
        assert_eq!(fs::read(&report).unwrap(), previous_report);
        assert_eq!(
            records(&fs::read(project.0.join("attempts.jsonl")).unwrap()).len(),
            2
        );
        if kind == "dangling" {
            fs::remove_file(&bundle).unwrap();
        } else {
            fs::remove_dir(&bundle).unwrap();
        }
    }
}

#[test]
fn repeated_legacy_default_keeps_each_replica_source_after_result_rotation() {
    let project = Project::new();
    project.config(2);
    fs::write(project.0.join("sweep.toml"), "configs=['benchmark.toml']\n").unwrap();
    let directory = project.0.join("sweep-results/000-benchmark/cassettes");
    fs::create_dir_all(&directory).unwrap();
    let source: Value =
        serde_json::from_slice(&fs::read(project.0.join("input.jsonl")).unwrap()).unwrap();
    for replica in 1..=2 {
        let mut interaction = source.clone();
        interaction["response"]["body"] = json!(json!({"recorded_track":100+replica}).to_string());
        fs::write(
            directory.join(format!(
                "{}-concurrency-{replica}.jsonl",
                legacy_name(&project)
            )),
            format!("{interaction}\n"),
        )
        .unwrap();
    }
    for arguments in [vec!["--replay", "--repeat", "2"], vec!["--replay"]] {
        let replayed = sweep(&project, None, &arguments);
        assert!(
            replayed.status.success(),
            "{}",
            String::from_utf8_lossy(&replayed.stderr)
        );
        let plan = campaign_plan(&replayed);
        let plan: Value = serde_json::from_slice(&fs::read(plan).unwrap()).unwrap();
        assert!(
            plan["replay_source"]
                .as_str()
                .unwrap()
                .contains("sweep-history")
        );
        assert!(plan["replay_selection"].is_null());
    }
    let attempts = records(&fs::read(project.0.join("attempts.jsonl")).unwrap());
    assert_eq!(attempts.len(), 6);
    for attempt in attempts {
        assert_eq!(
            attempt["response_tag"].as_u64(),
            attempt["replica"].as_u64().map(|replica| 100 + replica)
        );
    }
}
