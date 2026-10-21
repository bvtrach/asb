//! Provider-free parser, file-validation and command-contract fixtures.
use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bpftrace-fixtures")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn aggregate_and_series_reject_noise_and_keep_last_and_all_observations() {
    let output = "\nASB_BPFTRACE_READY\n@other: 999\n@asb_tree: 7\n@asb_tree[123]: 1\n@asb_invalid: nonsense\n@asb_nan: NaN\n@asb_infinity: inf\n  @asb_events: 2\n@asb_events: 5\n@asb_duration_ns: 2.5\n@asb_zero: 0\n";
    let metrics = parse_bpftrace_metrics("socket-operations", output);
    assert_eq!(metrics.len(), 3);
    assert_eq!(metrics["bpftrace.socket-operations.events"], 5.0);
    assert_eq!(metrics["bpftrace.socket-operations.duration_ns"], 2.5);
    assert_eq!(metrics["bpftrace.socket-operations.zero"], 0.0);
    let series = parse_bpftrace_series("socket-operations", output);
    assert_eq!(series.len(), 3);
    assert_eq!(series["bpftrace.socket-operations.events"], [2.0, 5.0]);
    assert!(parse_bpftrace_metrics("custom", "").is_empty());
    assert!(parse_bpftrace_series("custom", "@host: 10").is_empty());
}

#[test]
fn custom_source_validation_reports_file_bounds_and_missing_probes() {
    let fixture = Fixture::new();
    let path = fixture.0.join("program.bt");
    let error = validate_custom_program_scope(&path).unwrap_err();
    assert!(error.contains("cannot read"));
    assert!(error.contains("program.bt"));
    fs::write(&path, [0xff, 0xfe]).unwrap();
    assert!(
        validate_custom_program_scope(&path)
            .unwrap_err()
            .contains("not UTF-8")
    );
    fs::write(&path, vec![b' '; MAX_CUSTOM_PROGRAM as usize + 1]).unwrap();
    let error = validate_custom_program_scope(&path).unwrap_err();
    assert!(error.contains("too large"));
    assert!(error.contains("262144"));
    for source in [
        "",
        "// tracepoint:x:y /@asb_tree[pid]/ {}\n",
        "BEGIN { @seed = 1; }\nEND { clear(@seed); }\n",
    ] {
        fs::write(&path, source).unwrap();
        assert!(
            validate_custom_program_scope(&path)
                .unwrap_err()
                .contains("no process-tree-scoped probe")
        );
    }
    fs::write(&path, "interval:s:1 { @asb_ticks = count(); }\n").unwrap();
    assert!(
        validate_custom_program_scope(&path)
            .unwrap_err()
            .contains("no process-tree-scoped probe")
    );
    fs::write(&path, "BEGIN { @seed = 1; }\ntracepoint:syscalls:sys_enter_read /@asb_tree[pid]/ { @asb_reads = count(); } // bounded\nEND { clear(@seed); }\n").unwrap();
    validate_custom_program_scope(&path).unwrap();
    let scoped = scoped_custom_program(&path, std::process::id()).unwrap();
    assert!(scoped.contains(&format!("@asb_tree[{}] = 1;", std::process::id())));
    assert!(scoped.contains("args->parent_pid"));
    assert!(scoped.ends_with(&fs::read_to_string(&path).unwrap()));
}

#[test]
fn svg_profiles_explain_empty_results_escape_text_and_propagate_write_failure() {
    let fixture = Fixture::new();
    let metrics = BTreeMap::from([("bpftrace.io-wait.events".into(), 23.0)]);
    write_bpftrace_profile_svg(&fixture.0, "io-wait", "completed", &metrics).unwrap();
    let svg = fs::read_to_string(fixture.0.join("bpftrace-io-wait.svg")).unwrap();
    assert!(svg.contains("events: 23"));
    assert!(svg.contains("scheduler statistics"));
    assert!(svg.contains("#78dba9"));
    write_bpftrace_profile_svg(&fixture.0, "custom<&", "failed<&", &BTreeMap::new()).unwrap();
    let svg = fs::read_to_string(fixture.0.join("bpftrace-custom<&.svg")).unwrap();
    assert!(svg.contains("custom&lt;&amp;"));
    assert!(svg.contains("failed&lt;&amp;"));
    assert!(svg.contains("Custom process-tree profile"));
    assert!(svg.contains("no aggregate values emitted"));
    assert!(svg.contains("#ffb86b"));
    let error =
        write_bpftrace_profile_svg(&fixture.0.join("missing"), "io-wait", "failed", &metrics)
            .unwrap_err();
    assert!(error.contains("cannot write bpftrace profile SVG"));
}

#[test]
fn adapter_metadata_only_recognizes_owned_profile_artifacts() {
    assert_eq!(ADAPTER.id(), "bpftrace");
    assert_eq!(ADAPTER.tool_id(), Some("bpftrace"));
    assert_eq!(ADAPTER.environment_flag(), Some("ASB_MEASURE_BPFTRACE"));
    assert!(ADAPTER.requires_pid());
    assert!(ADAPTER.graceful_shutdown("bpftrace-custom"));
    assert!(ADAPTER.prioritizes_artifact("bpftrace-custom.json"));
    assert!(!ADAPTER.prioritizes_artifact("perfetto-custom.svg"));
    assert!(ADAPTER.display_artifact("bpftrace-custom.svg").is_some());
    assert!(ADAPTER.display_artifact("bpftrace-custom.json").is_none());
    assert!(ADAPTER.display_artifact("foreign.svg").is_none());
    assert_eq!(
        ADAPTER
            .graph_section("metric-bpftrace-custom")
            .unwrap()
            .title,
        "bpftrace profiles"
    );
    assert!(ADAPTER.graph_section("series-bpftrace-custom").is_some());
    assert!(ADAPTER.graph_section("metric-perf-custom").is_none());
}

#[cfg(unix)]
#[test]
fn process_hook_exports_profile_results_without_attaching_a_collector() {
    use std::os::unix::process::ExitStatusExt;
    let fixture = Fixture::new();
    // An execution-namespace sampler has no external executable/preflight and
    // does not start sampling until attach_pid. No host tracing is involved.
    let mut session =
        measurements::Session::start_in_execution_namespace(&fixture.0, &["agent-memory".into()])
            .unwrap();
    let status = std::process::ExitStatus::from_raw(0);
    let context = ProcessContext {
        name: "bpftrace-custom",
        status: &status,
        stdout: "@asb_events: 2\n@asb_events: 7\n",
        stderr: "",
        stderr_path: &fixture.0.join("bpftrace-custom.stderr"),
    };
    let result = ADAPTER.process(&mut session, &context).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.metrics["bpftrace.custom.events"], 7.0);
    assert_eq!(result.series["bpftrace.custom.events"], [2.0, 7.0]);
    assert!(fixture.0.join("bpftrace-custom.svg").is_file());
    let failed = std::process::ExitStatus::from_raw(2 << 8);
    let context = ProcessContext {
        name: "unrelated",
        status: &failed,
        stdout: "@asb_events: 9",
        stderr: "error",
        stderr_path: &fixture.0.join("unrelated.stderr"),
    };
    let result = ADAPTER.process(&mut session, &context).unwrap();
    assert_eq!(result.status, "failed");
    assert!(result.metrics.is_empty());
    assert!(result.series.is_empty());
}

/// Each environment configuration belongs to a separate single-test subprocess;
/// no process-wide environment mutation races parallel tests.
#[test]
fn isolated_profile_and_custom_program_selection() {
    const MARKER: &str = "ASB_BPFTRACE_TEST_ISOLATED";
    if std::env::var_os(MARKER).is_none() {
        for (case, profiles, programs) in [
            ("baseline", None, None),
            (
                "profiles",
                Some(" io-wait, ,socket-operations,io-wait "),
                None,
            ),
            ("all", Some("all"), None),
            ("missing-equals", None, Some("file.bt?count")),
            ("empty-key", None, Some("file.bt?=3")),
            ("empty-value", None, Some("file.bt?count=")),
            (
                "parameters",
                None,
                Some(";first.bt?ASB_PARAM_LIMIT=3&&ASB_PARAM_UNIT_NS=1;;second.bt;"),
            ),
            ("empty", None, Some(" ;;")),
        ] {
            let module = module_path!().split_once("::").unwrap().1;
            let test_name = format!("{module}::isolated_profile_and_custom_program_selection");
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &test_name, "--nocapture"])
                .env(MARKER, case)
                .env_remove("ASB_BPFTRACE_PROFILES")
                .env_remove("ASB_BPFTRACE_PROGRAMS");
            if let Some(profiles) = profiles {
                command.env("ASB_BPFTRACE_PROFILES", profiles);
            }
            if let Some(programs) = programs {
                command.env("ASB_BPFTRACE_PROGRAMS", programs);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        return;
    }
    match std::env::var(MARKER).unwrap().as_str() {
        "profiles" => {
            assert_eq!(selected_bpftrace_profiles(&[]).unwrap().len(), 2);
            return;
        }
        "all" => {
            assert_eq!(
                selected_bpftrace_profiles(&[]).unwrap().len(),
                BPFTRACE_PROFILES.len()
            );
            return;
        }
        "missing-equals" => {
            assert!(
                custom_programs()
                    .unwrap_err()
                    .contains("use PROGRAM?ASB_PARAM_LIMIT=12")
            );
            return;
        }
        "empty-key" | "empty-value" => {
            assert!(
                custom_programs()
                    .unwrap_err()
                    .contains("invalid bpftrace parameter")
            );
            return;
        }
        "parameters" => {
            assert_eq!(
                custom_programs().unwrap(),
                vec![
                    (
                        "first.bt".into(),
                        vec![
                            ("ASB_PARAM_LIMIT".into(), "3".into()),
                            ("ASB_PARAM_UNIT_NS".into(), "1".into())
                        ]
                    ),
                    ("second.bt".into(), vec![])
                ]
            );
            return;
        }
        "empty" => {
            assert!(custom_programs().unwrap().is_empty());
            return;
        }
        "baseline" => {}
        case => panic!("unknown fixture case {case}"),
    }
    assert!(selected_bpftrace_profiles(&[]).unwrap().is_empty());
    assert!(
        custom_programs()
            .unwrap_err()
            .contains("requires ASB_BPFTRACE_PROGRAMS")
    );
    let default = selected_bpftrace_profiles(&["bpftrace".into()]).unwrap();
    assert_eq!(default.len(), 5);
    assert!(default.iter().all(|p| p.default_selection));
    let selected = selected_bpftrace_profiles(&[
        "bpftrace:io-wait".into(),
        "bpftrace:io-wait".into(),
        "bpftrace:syscall-rate".into(),
    ])
    .unwrap();
    assert_eq!(
        selected.iter().map(|p| p.name).collect::<Vec<_>>(),
        ["io-wait", "syscall-rate"]
    );
    let error = selected_bpftrace_profiles(&["bpftrace:missing-profile".into()])
        .err()
        .unwrap();
    assert!(error.contains("unknown bpftrace profile `missing-profile`"));
    assert!(error.contains("socket-operations"));
}

#[cfg(unix)]
#[test]
fn profile_probe_obeys_executable_outcome_without_kernel_collection() {
    let fixture = Fixture::new();
    let program = profile_probe_program(BPFTRACE_PROFILES[0]);
    assert!(program.contains("@asb_tree[0] = 1;"));
    assert!(program.contains("interval:ms:1 { exit(); }"));
    // Immutable host fixtures avoid transient ETXTBSY when another libtest
    // thread forks while a newly-created script's writer is still inherited.
    assert!(bpftrace_profile_probe(Path::new("/bin/true"), BPFTRACE_PROFILES[0], false).unwrap());
    assert!(!bpftrace_profile_probe(Path::new("/bin/false"), BPFTRACE_PROFILES[0], false).unwrap());
    let error = bpftrace_profile_probe(&fixture.0.join("missing"), BPFTRACE_PROFILES[0], false)
        .unwrap_err();
    assert!(error.contains("cannot probe bpftrace profile sched-latency"));
}
