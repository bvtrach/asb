use super::*;
use std::process::Stdio;

struct FailingWriter(bool);
impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0 {
            Ok(bytes.len())
        } else {
            Err(io::Error::other("fixture write failed"))
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("fixture flush failed"))
    }
}

/// DescendantScope deliberately owns all of a supervisor's children. Isolate
/// each scenario from concurrent libtest workers and their unrelated children.
#[test]
fn isolated_workload_process_capture_timeout_and_failure_cleanup() {
    const CASE: &str = "ASB_WORKLOAD_PROCESS_TEST_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let name = format!(
            "{}::isolated_workload_process_capture_timeout_and_failure_cleanup",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "normal",
            "nonzero",
            "timeout",
            "missing-program",
            "missing-stdout",
            "missing-stderr",
            "write-failure",
            "flush-failure",
            "callback-failure",
            "truncated",
        ] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .env(CASE, case)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "case {case}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        return;
    };
    let mut command = Command::new("sh");
    command
        .args([
            "-c",
            "printf 'client stdout'; printf 'client stderr' >&2; exit 0",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut output = Vec::new();
    let mut errors = Vec::new();
    match case.as_str() {
        "normal" | "nonzero" => {
            if case == "nonzero" {
                command = Command::new("sh");
                command
                    .args(["-c", "printf 'failed output'; exit 7"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
            }
            let mut calls = 0;
            let held = start(
                &mut command,
                &mut output,
                &mut errors,
                Duration::from_secs(3),
                |status, elapsed, timed_out| {
                    calls += 1;
                    assert!(!timed_out);
                    assert!(elapsed < 3000);
                    assert_eq!(status.code(), Some(if case == "normal" { 0 } else { 7 }));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(calls, 1);
            assert!(!held.truncated);
            assert!(!held.timed_out);
            assert!(held.elapsed_ms < 3000);
            assert_eq!(held.output.stdout, output);
            assert_eq!(held.output.stderr, errors);
            if case == "normal" {
                assert_eq!(output, b"client stdout");
                assert_eq!(errors, b"client stderr");
            } else {
                assert_eq!(output, b"failed output");
            }
        }
        "timeout" => {
            let mut command = Command::new("sleep");
            command
                .arg("10")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut observed = false;
            let held = start(
                &mut command,
                &mut output,
                &mut errors,
                Duration::ZERO,
                |status, _, timed_out| {
                    assert!(timed_out);
                    assert!(!status.success());
                    observed = true;
                    Ok(())
                },
            )
            .unwrap();
            assert!(observed);
            assert!(held.timed_out);
            assert!(!held.output.status.success());
            assert!(String::from_utf8_lossy(&errors).contains("exceeded its time budget"));
        }
        "missing-program" => {
            let mut command = Command::new("/nonexistent-asb-owned-fixture-program");
            assert!(
                start(
                    &mut command,
                    &mut output,
                    &mut errors,
                    Duration::from_secs(1),
                    |_, _, _| Ok(())
                )
                .err()
                .unwrap()
                .to_string()
                .contains("cannot start workload client")
            );
        }
        "missing-stdout" | "missing-stderr" => {
            if case == "missing-stdout" {
                command.stdout(Stdio::null());
            } else {
                command.stderr(Stdio::null());
            }
            let error = start(
                &mut command,
                &mut output,
                &mut errors,
                Duration::from_secs(1),
                |_, _, _| Ok(()),
            )
            .err()
            .unwrap()
            .to_string();
            assert!(error.contains(if case == "missing-stdout" {
                "stdout is not piped"
            } else {
                "stderr is not piped"
            }));
        }
        "write-failure" | "flush-failure" => {
            let error = start(
                &mut command,
                &mut FailingWriter(case == "flush-failure"),
                &mut errors,
                Duration::from_secs(1),
                |_, _, _| Ok(()),
            )
            .err()
            .unwrap()
            .to_string();
            assert!(
                error.contains(if case == "write-failure" {
                    "fixture write failed"
                } else {
                    "fixture flush failed"
                }),
                "{error}"
            );
        }
        "callback-failure" => {
            let error = start(
                &mut command,
                &mut output,
                &mut errors,
                Duration::from_secs(1),
                |_, _, _| Err(Error::Run("fixture completion observer failed".into())),
            )
            .err()
            .unwrap()
            .to_string();
            assert!(error.contains("fixture completion observer failed"));
        }
        "truncated" => {
            let mut command = Command::new("python3");
            command
                .args([
                    "-I",
                    "-S",
                    "-B",
                    "-c",
                    "import sys; sys.stdout.buffer.write(b'x'*(8*1024*1024+100))",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let held = start(
                &mut command,
                &mut output,
                &mut errors,
                Duration::from_secs(10),
                |_, _, _| Ok(()),
            )
            .unwrap();
            assert!(held.truncated);
            assert_eq!(held.output.stdout.len(), LIMIT);
            assert_eq!(output.len(), LIMIT);
        }
        _ => panic!("invalid fixture case"),
    }
}
