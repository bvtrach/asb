//! Isolated bridge/proxy protocol doubles; no real bridge, auth files or provider.
use super::*;
use std::fs;
use std::path::Path;

#[cfg(unix)]
#[test]
fn bridge_configuration_and_startup_failures_are_isolated_and_owned() {
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_COVERAGE_BRIDGE_CASE";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/bridge-lifecycle-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let python = crate::tools::host_executable(Path::new("python3")).unwrap();
        let name = format!(
            "{}::bridge_configuration_and_startup_failures_are_isolated_and_owned",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "external",
            "missing-key",
            "empty-key",
            "invalid-endpoint",
            "missing-executable",
            "zero-port",
            "bad-port",
            "closed",
            "noise",
            "proxy-closed",
            "proxy-noise",
            "ready",
            "override-key",
            "wildcard-host",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(&directory).unwrap();
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .env_clear()
                .current_dir(&directory)
                .env(CASE, case)
                .env("HOME", &directory)
                .env("PATH", &directory)
                .env("ASB_FIXTURE_PYTHON", &python)
                .args(["--exact", &name, "--nocapture"]);
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                command.env("LLVM_PROFILE_FILE", profile);
            }
            if matches!(
                case,
                "external" | "missing-key" | "empty-key" | "invalid-endpoint"
            ) {
                command.env(
                    "CLI2KEY_BASE_URL",
                    if case == "invalid-endpoint" {
                        "\n"
                    } else {
                        "http://127.0.0.1:1/"
                    },
                );
            }
            if matches!(case, "external" | "override-key") {
                command.env("CLI2KEY_API_KEY", "fixture-configured-key");
            }
            if case == "empty-key" {
                command.env("CLI2KEY_API_KEY", "   ");
            }
            if case == "zero-port" {
                command.env("CODEX_BRIDGE_PORT", "0");
            }
            if case == "bad-port" {
                command.env("CODEX_BRIDGE_PORT", "invalid");
            }
            if case == "wildcard-host" {
                command.env("CODEX_BRIDGE_HOST", "0.0.0.0");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    let root = std::env::current_dir().unwrap();
    let python = std::env::var("ASB_FIXTURE_PYTHON").unwrap();
    for (name, script) in [
        (
            "bridge",
            r#"import os,sys,time
from pathlib import Path
Path('bridge.pid').write_text(str(os.getpid()))
case=os.environ['ASB_COVERAGE_BRIDGE_CASE']
if case=='closed': sys.exit(0)
if case=='noise':
 for _ in range(64): print('fixture noise',flush=True)
else: print('Codex Bridge API key: fixture-discovered-key',flush=True)
while True: time.sleep(0.1)
"#,
        ),
        (
            "node",
            r#"import os,sys,time
from pathlib import Path
Path('proxy.pid').write_text(str(os.getpid()))
case=os.environ['ASB_COVERAGE_BRIDGE_CASE']
if case=='proxy-closed': sys.exit(0)
if case=='proxy-noise':
 print('not a ready marker',flush=True);sys.exit(0)
print('ASB cli2key proxy ready',flush=True)
while True: time.sleep(0.1)
"#,
        ),
    ] {
        let path = root.join(name);
        fs::write(&path, format!("#!{python}\n{script}")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let executable = root.join("bridge");
    let result = Bridge::connect(if case == "missing-executable" {
        None
    } else {
        executable.to_str()
    });
    match case.as_str() {
        "external" | "ready" | "override-key" | "wildcard-host" => {
            let bridge = result.unwrap();
            let expected_key = if case == "external" || case == "override-key" {
                "fixture-configured-key"
            } else {
                "fixture-discovered-key"
            };
            assert_eq!(bridge.api_key(), expected_key);
            assert!(bridge.base_url().starts_with("http://127.0.0.1:"));
            assert!(bridge.base_url().ends_with("/v1"));
            if case == "external" {
                assert!(!root.join("bridge.pid").exists());
            }
            drop(bridge);
        }
        other => {
            let error = match result {
                Ok(_) => panic!("{other} unexpectedly connected"),
                Err(error) => error.to_string(),
            };
            let expected = match other {
                "missing-key" | "empty-key" => "CLI2KEY_API_KEY is missing",
                "invalid-endpoint" => "single-line endpoint",
                "missing-executable" => "needs a local bridge",
                "zero-port" | "bad-port" => "integer from 1 to 65535",
                "closed" => "startup output",
                "noise" => "did not provide an API key",
                "proxy-closed" | "proxy-noise" => "proxy did not become ready",
                _ => panic!("unknown case"),
            };
            assert!(error.contains(expected), "{other}: {error}");
        }
    }
    for name in ["bridge.pid", "proxy.pid"] {
        if let Ok(pid) = fs::read_to_string(root.join(name)) {
            assert!(
                !Path::new("/proc").join(pid.trim()).exists(),
                "owned {name} was not reaped"
            );
        }
    }
    let mut command = Command::new("not-executed");
    assert!(prepare(&mut command, "fixture-model", None).is_err());
    prepare(
        &mut command,
        "fixture-model",
        Some(("fixture-key", "http://127.0.0.1:1/v1")),
    )
    .unwrap();
    let env = command
        .get_envs()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        env[std::ffi::OsStr::new("ANTHROPIC_BASE_URL")].unwrap(),
        "http://127.0.0.1:1"
    );
    assert_eq!(
        env[std::ffi::OsStr::new("OPENAI_MODEL")].unwrap(),
        "fixture-model"
    );
}
