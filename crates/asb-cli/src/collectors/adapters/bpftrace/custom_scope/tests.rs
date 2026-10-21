use super::*;
use crate::collectors::CollectorAdapter;

fn guarded(predicate: &str) -> String {
    format!("kprobe:do_sys_open /{predicate}/ {{ @asb_events=count(); }}")
}

#[test]
fn custom_program_specifications_reject_duplicates_legacy_names_and_resource_excess() {
    use super::super::parse_custom_programs;
    let valid = parse_custom_programs("/one.bt?ASB_PARAM_LIMIT=12&ASB_PARAM_NS=0;/two.bt").unwrap();
    assert_eq!(valid.len(), 2);
    assert_eq!(valid[0].1[0], ("ASB_PARAM_LIMIT".into(), "12".into()));
    for (specification, expected) in [
        (
            "/one?ASB_PARAM_LIMIT=1&ASB_PARAM_LIMIT=2".into(),
            "duplicate",
        ),
        ("/one?limit=12".into(), "Legacy names"),
        ("/one?ASB_PARAM_LIMIT".into(), "ASB_PARAM_LIMIT=12"),
        ("?ASB_PARAM_LIMIT=1".into(), "path is empty"),
        ("x".repeat(16385), "16 KiB"),
        (["/one"; 9].join(";"), "at most 8"),
        (
            format!(
                "/one?{}",
                (0..33)
                    .map(|n| format!("ASB_PARAM_N{n}=1"))
                    .collect::<Vec<_>>()
                    .join("&")
            ),
            "at most 32",
        ),
    ] {
        assert!(
            parse_custom_programs(&specification)
                .unwrap_err()
                .contains(expected),
            "{expected}"
        );
    }
}

#[test]
fn complete_probe_blocks_and_positive_boolean_guards_are_supported() {
    for predicate in [
        "@asb_tree[pid]",
        "@asb_tree[tid] != 0",
        "@asb_tree[args->pid] > 0",
        "@asb_tree[args->prev_pid] >= 1",
        "@asb_tree[args->next_pid] == 1",
        "@asb_tree[args->parent_pid]",
        "@asb_tree[args->child_pid]",
        "(@asb_tree[pid]) && cpu > 0",
        "cpu > 0 && (@asb_tree[pid] || @asb_tree[tid])",
        "(@asb_tree[pid] && cpu == 0) || (@asb_tree[tid] && cpu != 0)",
        "@asb_tree[pid] || 0",
        "false || @asb_tree[pid]",
        "@asb_tree[pid] && strncmp(comm, \"codex\", 5) == 0",
        "@asb_tree[pid] && str(args->filename) != \"++\"",
    ] {
        assert!(validate(&guarded(predicate)).is_ok(), "{predicate}");
    }
    for source in [
        "/* λ { fake } */ kprobe:do_sys_open\n/@asb_tree[pid]/\n{ if (pid) { @asb_x=count(); } }",
        "BEGIN { @seed=1; } interval:s:1 { print(@asb_x); } kprobe:do_sys_open /@asb_tree[pid]/ { @asb_x=count(); } END { clear(@seed); }",
        "struct Foo { int n; }; union Bar { int x; int y; }; enum Flag { A, B }; kprobe:do_sys_open /@asb_tree[pid]/ { @asb_x=count(); }",
        "uprobe:/usr/bin/program:main /@asb_tree[pid]/ { @asb_x=count(); }",
        "uprobe:\"/tmp/λ program\":main /@asb_tree[pid]/ { @asb_x=count(); }",
        "kprobe:do_sys_open /@asb_tree[pid]/ { printf(\"not @asb_tree { \\\" }\"); @asb_tree_extra=1; }",
        "kprobe:do_sys_open /@asb_tree[pid]/ { @asb_x=count(); }\n// unscoped:fake {}",
    ] {
        assert!(validate(source).is_ok(), "{source}");
    }
}

#[test]
fn multiline_unscoped_handlers_macros_and_opaque_membership_do_not_pass() {
    for source in [
        "",
        "BEGIN { @x=1; }",
        "interval:s:1 { @x=count(); }",
        "kprobe:do_sys_open\n{ @host=count(); }",
        "kprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }\nkprobe:do_sys_read\n{ @host=count(); }",
        "BEGIN,kprobe:do_sys_open { @x=count(); }",
        "// @asb_tree[pid]\nkprobe:do_sys_open { @host=count(); }",
        "kprobe:do_sys_open /\"@asb_tree[pid]\"/ { @host=count(); }",
        "#define SCOPED @asb_tree[pid]\nkprobe:do_sys_open /SCOPED/ { @host=count(); }",
        "#define LIMIT 10\nkprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }",
        "struct Incomplete { int x; } kprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }",
        "kprobe:do_sys_open /@asb_tree[pid]/ { clear(@asb_tree); }",
        "BEGIN { @asb_tree[pid]=1; } kprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }",
        "END { delete(@asb_tree[pid]); } kprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }",
        "interval:s:1 { zero(@asb_tree); } kprobe:do_sys_open /@asb_tree[pid]/ { @x=count(); }",
        "kprobe:do_sys_open /@asb_tree[pid]/ {",
        "kprobe:do_sys_open /@asb_tree[pid]/ { printf(\"unterminated); }",
        "kprobe:do_sys_open /@asb_tree[pid]/ { /* unterminated",
        "kprobe:do_sys_open",
    ] {
        assert!(validate(source).is_err(), "accepted {source}");
    }
    for predicate in [
        "@asb_tree[pid] || 1",
        "1 || @asb_tree[pid]",
        "@asb_tree[pid] || cpu == 0",
        "!@asb_tree[pid]",
        "@other[pid]",
        "@asb_tree_suffix[pid]",
        "@asb_tree[uid]",
        "@asb_tree[pid] == 0",
        "@asb_tree[pid] < 1",
        "@asb_tree[pid] | 1",
        "@asb_tree[pid] ? 1 : 1",
        "@asb_tree[pid] &&",
        "|| @asb_tree[pid]",
        "(@asb_tree[pid]",
        "@asb_tree[pid]]",
        "(@asb_tree[pid]) || (1)",
    ] {
        assert!(
            validate(&guarded(predicate)).is_err(),
            "accepted {predicate}"
        );
    }
}

#[test]
fn predicate_mutations_are_rejected_before_positive_scope_proof() {
    for effect in [
        "++@asb_tree[pid]",
        "--@asb_tree[pid]",
        "@asb_tree[pid]++",
        "@asb_tree[pid]--",
        "(@asb_tree[pid]=1)",
        "(@asb_tree[pid] += 1)",
        "(@asb_tree[pid] -= 1)",
        "(@asb_tree[pid] *= 2)",
        "(@asb_tree[pid] /= 2)",
        "(@asb_tree[pid] %= 2)",
        "(@asb_tree[pid] &= 1)",
        "(@asb_tree[pid] |= 1)",
        "(@asb_tree[pid] ^= 1)",
        "(@asb_tree[pid] <<= 1)",
        "(@asb_tree[pid] >>= 1)",
        "delete(@asb_tree[pid])",
        "clear(@asb_tree)",
        "zero(@asb_tree)",
        "print(@asb_tree)",
        "system(\"true\")",
        "exit()",
        "unknown_call(pid)",
        "str(++@asb_tree[pid])",
        "($variable++)",
    ] {
        for predicate in [
            format!("{effect} && @asb_tree[pid]"),
            format!("@asb_tree[pid] && ({effect})"),
            format!("({effect} || @asb_tree[pid]) && @asb_tree[pid]"),
        ] {
            assert!(
                validate(&guarded(&predicate)).is_err(),
                "mutation accepted: {predicate}"
            );
        }
    }
}

#[test]
fn boolean_scope_proof_never_admits_an_unrelated_task_in_small_truth_tables() {
    let atoms = ["@asb_tree[pid]", "cpu > 0", "0"];
    for a in 0..3 {
        for b in 0..3 {
            for c in 0..3 {
                for first_and in [true, false] {
                    for second_and in [true, false] {
                        let predicate = format!(
                            "({} {} {}) {} {}",
                            atoms[a],
                            if first_and { "&&" } else { "||" },
                            atoms[b],
                            if second_and { "&&" } else { "||" },
                            atoms[c]
                        );
                        if validate(&guarded(&predicate)).is_ok() {
                            for unknown in [false, true] {
                                let values = [false, unknown, false]; // membership is false
                                let first = if first_and {
                                    values[a] && values[b]
                                } else {
                                    values[a] || values[b]
                                };
                                let result = if second_and {
                                    first && values[c]
                                } else {
                                    first || values[c]
                                };
                                assert!(
                                    !result,
                                    "scope proof admitted unrelated task: {predicate}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    let deep = format!("{}@asb_tree[pid]{}", "(".repeat(80), ")".repeat(80));
    assert!(validate(&guarded(&deep)).is_err());
    let oversized = format!(
        "kprobe:{} /@asb_tree[pid]/ {{ @x=count(); }}",
        "x".repeat(8192)
    );
    assert!(validate(&oversized).unwrap_err().contains("8192"));
}

#[test]
fn numeric_parameter_names_values_and_token_boundaries_are_unambiguous() {
    for value in ["0", "1", "12", "9223372036854775807"] {
        parameter("ASB_PARAM_LIMIT", value).unwrap();
    }
    for key in [
        "pid",
        "ASB_LIMIT",
        "ASB_PARAM_",
        "ASB_PARAM_0BAD",
        "ASB_PARAM_lower",
        "ASB_PARAM_λ",
        "ASB_PARAM_X-Y",
    ] {
        assert!(parameter(key, "1").is_err(), "{key}");
    }
    assert!(parameter(&format!("ASB_PARAM_{}", "X".repeat(64)), "1").is_err());
    for value in [
        "",
        "00",
        "01",
        "-1",
        "+1",
        "1 ",
        " 1",
        "0xff",
        "1e3",
        "NaN",
        "1;BEGIN{}",
        "9223372036854775808",
        "18446744073709551616",
    ] {
        assert!(parameter("ASB_PARAM_LIMIT", value).is_err(), "{value}");
    }
    let parameters = vec![("ASB_PARAM_LIMIT".into(), "12".into())];
    let original = "// λ ASB_PARAM_LIMIT\n@ASB_PARAM_LIMIT=1; $ASB_PARAM_LIMIT=2; args->ASB_PARAM_LIMIT; printf(\"ASB_PARAM_LIMIT\"); xASB_PARAM_LIMIT; @x=ASB_PARAM_LIMIT;";
    let bound = bind_parameters(original, &parameters).unwrap();
    assert!(bound.ends_with("@x=12;"));
    assert!(bound.starts_with("// λ ASB_PARAM_LIMIT\n@ASB_PARAM_LIMIT=1;"));
    assert!(bound.contains("$ASB_PARAM_LIMIT=2"));
    assert!(bound.contains("args->ASB_PARAM_LIMIT"));
    assert!(bound.contains("printf(\"ASB_PARAM_LIMIT\")"));
    assert!(bound.contains("xASB_PARAM_LIMIT"));
    for member in [
        "args-> ASB_PARAM_LIMIT",
        "args-> /* field */ ASB_PARAM_LIMIT",
        "$s . ASB_PARAM_LIMIT",
        "$s . /* field */ ASB_PARAM_LIMIT",
    ] {
        assert!(
            bind_parameters(member, &parameters)
                .unwrap_err()
                .contains("not referenced")
        );
        assert_eq!(
            bind_parameters(&format!("{member}; @x=ASB_PARAM_LIMIT;"), &parameters).unwrap(),
            format!("{member}; @x=12;")
        );
    }
    for expression in ["pid>ASB_PARAM_LIMIT", "pid > ASB_PARAM_LIMIT"] {
        assert_eq!(
            bind_parameters(expression, &parameters).unwrap(),
            expression.replace("ASB_PARAM_LIMIT", "12")
        );
    }
    assert!(
        bind_parameters("ASB_PARAM_UNKNOWN", &parameters)
            .unwrap_err()
            .contains("missing")
    );
    assert!(
        bind_parameters("// ASB_PARAM_LIMIT", &parameters)
            .unwrap_err()
            .contains("not referenced")
    );
    assert!(
        bind_parameters(
            "ASB_PARAM_LIMIT",
            &[parameters[0].clone(), parameters[0].clone()]
        )
        .unwrap_err()
        .contains("duplicate")
    );
    assert!(
        bind_parameters(
            "ASB_PARAM_LIMIT",
            &[("ASB_PARAM_LIMIT".into(), "1; @asb_tree[pid]=1".into())]
        )
        .is_err()
    );
}

#[test]
fn numeric_binding_never_retargets_probe_attachment_tokens() {
    let parameters = [("ASB_PARAM_LIMIT".into(), "12".into())];
    for attachment in [
        "uprobe:/tmp/ASB_PARAM_LIMIT:main",
        "uprobe:\"/tmp/ASB_PARAM_LIMIT\":main",
        "uprobe:/tmp/λ/ASB_PARAM_LIMIT:ASB_PARAM_LIMIT",
        "kprobe:ASB_PARAM_LIMIT",
        "ASB_PARAM_LIMIT:some_event",
        "usdt:/tmp/ASB_PARAM_LIMIT:ASB_PARAM_LIMIT:probe",
        "kprobe:ASB_PARAM_LIMIT,kprobe:another",
    ] {
        let source = format!(
            "{attachment} /@asb_tree[pid] && pid > ASB_PARAM_LIMIT/ {{ if (ASB_PARAM_LIMIT > 0) {{ @x=count(); }} }}"
        );
        let bound = bind_parameters(&source, &parameters).unwrap();
        assert_eq!(
            bound,
            format!(
                "{attachment} /@asb_tree[pid] && pid > 12/ {{ if (12 > 0) {{ @x=count(); }} }}"
            )
        );
        validate(&bound).unwrap();
        let unused = format!("{attachment} /@asb_tree[pid]/ {{ @x=count(); }}");
        assert!(
            bind_parameters(&unused, &parameters)
                .unwrap_err()
                .contains("not referenced")
        );
    }
    let multiple = "struct Foo { int value; }; uprobe:/tmp/ASB_PARAM_LIMIT:main /@asb_tree[pid]/ { @x=ASB_PARAM_LIMIT; }\nkprobe:ASB_PARAM_LIMIT /@asb_tree[pid]/ { @y=ASB_PARAM_LIMIT; }";
    let bound = bind_parameters(multiple, &parameters).unwrap();
    assert!(bound.contains("uprobe:/tmp/ASB_PARAM_LIMIT:main"));
    assert!(bound.contains("kprobe:ASB_PARAM_LIMIT"));
    assert!(bound.contains("@x=12"));
    assert!(bound.contains("@y=12"));
}

#[test]
fn custom_source_read_is_bounded_regular_utf8_and_reports_its_path() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/custom-bpftrace-fixtures")
        .join(crate::workloads::TaskIdentity::local("custom-scope", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("program.bt");
    assert!(read(&path, &[]).unwrap_err().contains("program.bt"));
    assert!(read(&root, &[]).unwrap_err().contains("not a regular file"));
    std::fs::write(&path, [0xff]).unwrap();
    assert!(read(&path, &[]).unwrap_err().contains("not UTF-8"));
    std::fs::write(
        &path,
        vec![b' '; super::super::MAX_CUSTOM_PROGRAM as usize + 1],
    )
    .unwrap();
    assert!(read(&path, &[]).unwrap_err().contains("too large"));
    std::fs::write(&path, guarded("@asb_tree[pid]")).unwrap();
    let snapshot = read(&path, &[]).unwrap();
    std::fs::write(&path, "kprobe:do_sys_open { @host=count(); }").unwrap();
    assert!(validate(&snapshot).is_ok());
    assert!(read(&path, &[]).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn configuring_or_attaching_without_preflight_never_reloads_custom_programs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/custom-bpftrace-fixtures")
        .join(crate::workloads::TaskIdentity::local("missing-preparation", None).attempt_id);
    let mut session =
        crate::measurements::Session::start_in_execution_namespace(&root, &["agent-memory".into()])
            .unwrap();
    let adapter = &super::super::ADAPTER;
    assert!(
        adapter
            .configure(&mut session, &[])
            .unwrap_err()
            .contains("preparation state is missing")
    );
    assert!(
        adapter
            .attach_pid(&mut session, std::process::id())
            .unwrap_err()
            .contains("cannot re-read")
    );
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn installed_bpftrace_cli_accepts_the_bound_parameter_argument_route_without_attachment() {
    // -V exits during CLI processing, before root checks or probe attachment.
    // This qualifies the real argument route, NOT compilation or tracing.
    let explicit = std::env::var_os("ASB_TEST_BPFTRACE").map(std::path::PathBuf::from);
    let binary = explicit
        .clone()
        .unwrap_or_else(|| "/usr/bin/bpftrace".into());
    if explicit.is_none() && !binary.is_file() {
        eprintln!(
            "SKIP real bpftrace CLI check: /usr/bin/bpftrace absent; set ASB_TEST_BPFTRACE to an installed executable. Mock tests are not real-tool qualification."
        );
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/custom-bpftrace-fixtures")
        .join(crate::workloads::TaskIdentity::local("real-cli-options", None).attempt_id);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("program.bt");
    std::fs::write(&path, guarded("@asb_tree[pid] && ASB_PARAM_LIMIT > 0")).unwrap();
    let body = read(&path, &[("ASB_PARAM_LIMIT".into(), "12".into())]).unwrap();
    let prepared = super::super::PreparedCustomProgram {
        path: path.display().to_string(),
        source: body.into(),
    };
    let mut command = super::super::custom_command(
        std::process::Command::new(&binary),
        &prepared,
        &[std::process::id()],
    );
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &root)
        .arg("-V");
    let arguments = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>();
    assert!(!arguments.iter().any(|arg| arg == "-D"));
    assert!(arguments.iter().any(|arg| arg.contains("12 > 0")));
    let output = command
        .output()
        .expect("execute explicitly selected real bpftrace");
    assert!(
        output.status.success(),
        "{}: {}{}",
        binary.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version = String::from_utf8_lossy(&output.stdout);
    assert!(version.contains("bpftrace v"), "{version}");
    eprintln!(
        "REAL CLI OPTIONS ONLY: {} accepted bound numeric source without -D; {}",
        binary.display(),
        version.trim()
    );
    std::fs::remove_dir_all(root).unwrap();
}
