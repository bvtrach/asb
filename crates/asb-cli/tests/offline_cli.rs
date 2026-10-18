//! CLI contract tests using isolated local projects and no provider credentials.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "asb-offline-cli-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn command(&self, args: &[&str]) -> Output {
        self.command_with_env(args, &[])
    }

    fn command_with_env(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_asb"));
        command
            .current_dir(&self.0)
            .args(args)
            .env_clear()
            .env("HOME", &self.0)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        // Preserve instrumentation for coverage runs, without inheriting keys.
        if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
            command.env("LLVM_PROFILE_FILE", profile);
        }
        command.envs(extra.iter().copied());
        command.output().expect("ASB starts")
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn init(&self) {
        expect_success(&self.command(&["init"]));
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn expect_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        stdout(output),
        stderr(output)
    );
}
fn expect_failure(output: &Output, message: &str) {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        stdout(output)
    );
    assert!(
        stderr(output).contains(message),
        "expected {message:?}; stderr={}",
        stderr(output)
    );
}

#[test]
fn help_is_successful_for_each_noninteractive_command() {
    let project = Project::new();
    for command in [
        vec!["--help"],
        vec!["init", "--help"],
        vec!["setup", "--help"],
        vec!["run", "--help"],
        vec!["sweep", "--help"],
        vec!["compare", "--help"],
        vec!["provider", "--help"],
        vec!["provider", "models", "--help"],
        vec!["tool", "--help"],
        vec!["tool", "list", "--help"],
        vec!["tool", "install", "--help"],
        vec!["workload", "--help"],
        vec!["workload", "list", "--help"],
        vec!["workload", "install", "--help"],
    ] {
        let result = project.command(&command);
        expect_success(&result);
        assert!(stdout(&result).contains("Usage:"), "{command:?}");
        assert!(
            stderr(&result).is_empty(),
            "{command:?}: {}",
            stderr(&result)
        );
    }
}

#[test]
fn unknown_commands_suggest_the_intended_command() {
    let project = Project::new();
    for (args, suggestion) in [
        (vec!["sweeep"], "sweep"),
        (vec!["tool", "instal"], "install"),
        (vec!["workload", "lst"], "list"),
    ] {
        expect_failure(
            &project.command(&args),
            &format!("did you mean `{suggestion}`"),
        );
    }
}

#[test]
fn invalid_run_and_sweep_flags_fail_before_execution() {
    let project = Project::new();
    for (args, message) in [
        (
            vec!["run", "missing.toml", "--cassette", "x"],
            "--cassette requires",
        ),
        (
            vec!["run", "missing.toml", "--respect-record-timings"],
            "requires --replay",
        ),
        (
            vec!["sweep", "missing.toml", "--respect-record-timings"],
            "requires --replay",
        ),
        (
            vec!["run", "missing.toml", "--record", "--replay"],
            "cannot be used with",
        ),
        (
            vec!["sweep", "missing.toml", "--record", "--replay"],
            "cannot be used with",
        ),
    ] {
        expect_failure(&project.command(&args), message);
    }
    assert!(!project.0.join("asb-workspace").exists());
}

#[test]
fn setup_writes_a_parseable_user_selected_configuration() {
    let project = Project::new();
    project.init();
    let result = project.command(&[
        "setup",
        "--agent",
        "pi",
        "--workload",
        "bug-fix",
        "--provider",
        "cli2key",
        "--model",
        "gpt-6.1-sol",
        "--concurrency",
        "3",
        "--workspace",
        "my workspace",
        "--output",
        "selected.toml",
    ]);
    expect_success(&result);
    assert!(stdout(&result).contains("asb run selected.toml"));
    let text = std::fs::read_to_string(project.0.join("selected.toml")).unwrap();
    let config: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(config["agent"].as_str(), Some("pi"));
    assert_eq!(config["provider"].as_str(), Some("cli2key"));
    assert_eq!(config["model"].as_str(), Some("gpt-6.1-sol"));
    assert_eq!(config["concurrency"].as_integer(), Some(3));
    assert_eq!(config["workspace"].as_str(), Some("my workspace"));
    assert_eq!(config["log_file"].as_str(), Some("selected.log"));
    assert!(!text.to_ascii_lowercase().contains("api_key"));
}

#[test]
fn setup_rejects_missing_empty_or_zero_inputs_without_a_config_file() {
    let project = Project::new();
    project.init();
    let base = [
        "setup",
        "--agent",
        "pi",
        "--workload",
        "bug-fix",
        "--provider",
        "cli2key",
        "--model",
        "gpt-6.1-sol",
    ];
    expect_failure(&project.command(&["setup"]), "requires agent");
    let mut zero = base.to_vec();
    zero.extend(["--concurrency", "0", "--output", "bad.toml"]);
    expect_failure(&project.command(&zero), "at least 1");
    expect_failure(
        &project.command(&[
            "setup",
            "--agent",
            "",
            "--workload",
            "bug-fix",
            "--provider",
            "cli2key",
            "--model",
            "gpt-6.1-sol",
            "--output",
            "empty.toml",
        ]),
        "cannot be empty",
    );
    assert!(!project.0.join("bad.toml").exists());
    assert!(!project.0.join("empty.toml").exists());
}

#[test]
fn initialization_refuses_to_overwrite_an_existing_project() {
    let project = Project::new();
    project.init();
    project.write("workloads/custom.txt", "keep this prompt");
    let result = project.command(&["init"]);
    expect_failure(&result, "already initialized");
    assert_eq!(
        std::fs::read_to_string(project.0.join("workloads/custom.txt")).unwrap(),
        "keep this prompt"
    );
}

#[cfg(unix)]
#[test]
fn local_tool_install_copies_and_marks_an_executable_without_downloading() {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new();
    project.init();
    project.write("fixture-tool", "#!/bin/sh\nexit 0\n");
    expect_success(&project.command(&["tool", "install", "mytool", "--source", "fixture-tool"]));
    let installed = project.0.join(".asb-tools/mytool/bin/mytool");
    assert_eq!(
        std::fs::read_to_string(&installed).unwrap(),
        "#!/bin/sh\nexit 0\n"
    );
    assert_ne!(
        std::fs::metadata(installed).unwrap().permissions().mode() & 0o111,
        0
    );
}

#[test]
fn local_workload_install_and_list_use_the_project_registry() {
    let project = Project::new();
    project.init();
    project.write("prompt-source.txt", "Offline example workload\n");
    expect_success(&project.command(&[
        "workload",
        "install",
        "prompt-source.txt",
        "--name",
        "example",
    ]));
    assert_eq!(
        std::fs::read_to_string(project.0.join("workloads/example.txt")).unwrap(),
        "Offline example workload\n"
    );
    let result = project.command(&["workload", "list"]);
    expect_success(&result);
    assert!(stdout(&result).contains("example"));
}

#[test]
fn missing_local_install_sources_fail_without_provider_access() {
    let project = Project::new();
    project.init();
    let tool = project.command(&["tool", "install", "mytool", "--source", "missing-tool"]);
    assert!(!tool.status.success());
    assert!(!project.0.join(".asb-tools/mytool/bin/mytool").exists());
    let workload = project.command(&[
        "workload",
        "install",
        "missing-prompt.txt",
        "--name",
        "example",
    ]);
    assert!(!workload.status.success());
    assert!(!project.0.join("workloads/example.txt").exists());
}

#[test]
fn sweep_rejects_invalid_manifests_and_unsafe_references() {
    let project = Project::new();
    for (name, content, message) in [
        ("empty.toml", "configs = []\n", "contains no benchmark"),
        (
            "unknown.toml",
            "configs = []\nunknown = true\n",
            "unknown field",
        ),
        ("normal.toml", "agent = 'pi'\n", "not a sweep TOML"),
        (
            "missing.toml",
            "configs = ['missing-target.toml']\n",
            "references missing benchmark",
        ),
        (
            "traversal.toml",
            "configs = ['../outside.toml']\n",
            "unsafe benchmark path",
        ),
        (
            "extension.toml",
            "configs = ['bad.txt']\n",
            "not a .toml benchmark",
        ),
        (
            "absolute.toml",
            "configs = ['/tmp/other.toml']\n",
            "unsafe benchmark path",
        ),
    ] {
        project.write(name, content);
        expect_failure(&project.command(&["sweep", name]), message);
    }
}

#[test]
fn sweep_rejects_nested_sweeps_and_reports_comparison_failure() {
    let project = Project::new();
    project.init();
    project.write("outer.toml", "configs = ['inner.toml']\n");
    project.write("inner.toml", "configs = ['irrelevant.toml']\n");
    let result = project.command(&["sweep", "outer.toml"]);
    expect_failure(&result, "nested sweep");
    assert!(stderr(&result).contains("comparison failed"));
}

#[test]
fn missing_compare_input_fails_without_creating_a_report() {
    let project = Project::new();
    let result = project.command(&["compare", "missing.toml"]);
    assert!(!result.status.success());
    assert!(!project.0.join("results").exists());
}

#[cfg(unix)]
#[test]
fn ollama_discovery_and_codex_run_use_the_selected_local_endpoint() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let worker_stop = Arc::clone(&stop);
    let worker_requests = Arc::clone(&requests);
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fixture server: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..header_end]);
                    let size = header
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if bytes.len() >= header_end + 4 + size {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.lines().next().unwrap_or_default().to_owned();
            worker_requests.lock().unwrap().push(path.clone());
            let body = if path.starts_with("GET /api/tags ") {
                r#"{"models":[{"name":"qwen3:0.6b"},{"name":"unknown"},{"name":"no-tools","capabilities":["completion"]},{"name":"modern","capabilities":["completion","tools"]}]}"#
            } else if path.starts_with("POST /api/show ") {
                if request.contains("qwen3:0.6b") {
                    r#"{"capabilities":["completion","tools","thinking"]}"#
                } else {
                    assert!(request.contains("unknown"));
                    r#"{"capabilities":["completion"]}"#
                }
            } else if path.starts_with("POST /v1/responses ") {
                r#"{"id":"resp_fixture","object":"response","output":[]}"#
            } else {
                panic!("unexpected request: {path}")
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });

    let project = Project::new();
    project.init();
    let endpoint = format!("http://{address}");
    let host = address.to_string();
    let catalog =
        project.command_with_env(&["provider", "models", "ollama"], &[("OLLAMA_HOST", &host)]);
    expect_success(&catalog);
    assert!(stdout(&catalog).contains("qwen3:0.6b"));
    assert!(stdout(&catalog).contains("modern"));
    assert!(!stdout(&catalog).contains("no-tools"));
    assert!(!stdout(&catalog).contains("unknown"));
    project.write("benchmark.toml", "agent='codex'\nworkload='bug-fix'\nprovider='ollama'\nmodel='qwen3:0.6b'\nworkspace='workspace'\nlog_file='run.log'\n");
    let agent = ".asb-tools/codex/bin/codex";
    project.write(agent, r##"#!/usr/bin/env node
const fs = require('fs');
const cfg = fs.readFileSync(process.env.CODEX_HOME + '/config.toml', 'utf8');
const endpoint = cfg.match(/base_url = "([^"]+)"/)[1];
if (!cfg.includes('model_provider = "asb_provider"') || !cfg.includes('web_search = "disabled"')) process.exit(11);
const body = JSON.stringify({model:'qwen3:0.6b',input:'offline test',tools:[{type:'function',name:'read_file'}]});
fetch(endpoint + '/responses', {method:'POST',headers:{'content-type':'application/json'},body}).then(r => { if (!r.ok) process.exit(12); console.log('local-ollama-agent'); }).catch(() => process.exit(13));
"##);
    std::fs::set_permissions(
        project.0.join(agent),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let run = project.command_with_env(&["run", "benchmark.toml"], &[("OLLAMA_HOST", &endpoint)]);
    let record = project.command_with_env(
        &[
            "run",
            "benchmark.toml",
            "--record",
            "--cassette",
            "tape.jsonl",
        ],
        &[("OLLAMA_HOST", &endpoint)],
    );
    let replay = project.command_with_env(
        &[
            "run",
            "benchmark.toml",
            "--replay",
            "--cassette",
            "tape.jsonl",
        ],
        &[("OLLAMA_HOST", "http://127.0.0.1:1")],
    );
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    expect_success(&run);
    expect_success(&record);
    expect_success(&replay);
    assert!(
        std::fs::read_to_string(project.0.join("run.log"))
            .unwrap()
            .contains("local-ollama-agent")
    );
    let seen = requests.lock().unwrap();
    assert!(seen.iter().any(|path| path.starts_with("POST /api/show ")));
    assert!(
        seen.iter()
            .any(|path| path.starts_with("POST /v1/responses "))
    );
}

#[cfg(unix)]
#[test]
fn codex_uses_selected_openrouter_endpoint_for_live_record_and_replay() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use std::time::Duration;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let worker_stop = Arc::clone(&stop);
    let worker_requests = Arc::clone(&requests);
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fixture server: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.lines().next().unwrap_or_default().to_owned();
            worker_requests.lock().unwrap().push(request);
            let body = if path.starts_with("GET /api/v1/models ") {
                r#"{"data":[{"id":"fixture/model","supported_parameters":["tools"]}]}"#
            } else if path.starts_with("POST /api/v1/responses ") {
                r#"{"id":"resp_fixture","object":"response","output":[]}"#
            } else {
                panic!("unexpected request: {path}")
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let project = Project::new();
    project.init();
    project.write("benchmark.toml", "agent='codex'\nworkload='bug-fix'\nprovider='openrouter'\nmodel='fixture/model'\nworkspace='workspace'\nlog_file='run.log'\n");
    let agent = ".asb-tools/codex/bin/codex";
    project.write(agent, r##"#!/usr/bin/env node
const fs = require('fs');
const cfg = fs.readFileSync(process.env.CODEX_HOME + '/config.toml', 'utf8');
const endpoint = cfg.match(/base_url = "([^"]+)"/)[1];
if (!cfg.includes('model_provider = "asb_provider"') || !cfg.includes('wire_api = "responses"') || !cfg.includes('env_key = "ASB_AGENT_API_KEY"')) process.exit(11);
if (process.env.OPENROUTER_API_KEY === 'live-secret' && process.env.ASB_AGENT_API_KEY !== 'live-secret') process.exit(12);
if (process.env.OPENROUTER_API_KEY === 'asb-replay' && process.env.ASB_AGENT_API_KEY !== 'asb-replay') process.exit(13);
const body = JSON.stringify({model:'fixture/model',input:'offline test',tools:[{type:'function',name:'read_file'}]});
fetch(endpoint + '/responses', {method:'POST',headers:{'content-type':'application/json','authorization':'Bearer '+process.env.ASB_AGENT_API_KEY},body}).then(r => {if (!r.ok) process.exit(14); console.log('provider-neutral-agent');}).catch(() => process.exit(15));
"##);
    std::fs::set_permissions(
        project.0.join(agent),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let base = format!("http://{address}/api/v1");
    let env = [
        ("OPENROUTER_BASE_URL", base.as_str()),
        ("OPENROUTER_API_KEY", "live-secret"),
    ];
    let run = project.command_with_env(&["run", "benchmark.toml"], &env);
    let record = project.command_with_env(
        &[
            "run",
            "benchmark.toml",
            "--record",
            "--cassette",
            "tape.jsonl",
        ],
        &env,
    );
    let before_replay = requests.lock().unwrap().len();
    let replay = project.command_with_env(
        &[
            "run",
            "benchmark.toml",
            "--replay",
            "--cassette",
            "tape.jsonl",
        ],
        &env,
    );
    let after_replay = requests.lock().unwrap().len();
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    expect_success(&run);
    expect_success(&record);
    expect_success(&replay);
    assert_eq!(
        before_replay, after_replay,
        "replay contacted the live provider"
    );
    let seen = requests.lock().unwrap();
    let posts: Vec<_> = seen
        .iter()
        .filter(|request| request.starts_with("POST /api/v1/responses "))
        .collect();
    assert_eq!(posts.len(), 2);
    assert!(
        posts[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer live-secret")
    );
}

#[cfg(unix)]
fn replay_project(concurrency: usize) -> Project {
    use std::os::unix::fs::PermissionsExt;
    let project = Project::new();
    project.init();
    project.write("benchmark.toml", &format!(
        "agent='pi'\nworkload='bug-fix'\nprovider='cli2key'\nmodel='test-model'\nconcurrency={concurrency}\nworkspace='workspace'\nlog_file='run.log'\n"
    ));
    let body =
        serde_json::json!({"model":"test-model","input":"offline","tools":[{"type":"function"}]})
            .to_string();
    let cassette = serde_json::json!({"request":{"method":"POST","url":"/v1/responses","body":body},
        "response":{"status":200,"headers":{"content-type":"application/json"},"body":"{\"ok\":true}"}});
    project.write("tape.jsonl", &format!("{cassette}\n"));
    let agent = ".asb-tools/pi/bin/pi";
    project.write(agent, r##"#!/usr/bin/env node
const fs = require('fs');
const config = JSON.parse(fs.readFileSync(process.env.PI_CODING_AGENT_DIR + '/models.json'));
const endpoint = config.providers.asb.baseUrl;
if (!/^http:\/\/127\.0\.0\.1:\d+\/v1$/.test(endpoint) || config.providers.asb.api !== 'openai-responses' || process.env.ASB_AGENT_API_KEY !== 'asb-replay') {
  console.error('unexpected replay configuration'); process.exit(11);
}
const behavior = process.env.ASB_FIXTURE_BEHAVIOR || 'success';
async function main() {
  if (behavior === 'skip') { console.log('fixture-no-request'); return; }
  const body = {model:'test-model',input:behavior === 'mismatch' ? 'changed' : 'offline',tools:[{type:'function'}]};
  const send = () => fetch(endpoint + '/responses', {method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
  let response = await send();
  if (behavior === 'extra') response = await send();
  if (!response.ok) { console.error('fixture-rejected', response.status); process.exitCode = 8; return; }
  console.log('fixture-agent-output');
  if (behavior === 'fatal') console.error('Authentication required. Please use /login command to sign in to your account');
  if (behavior === 'exit' || (behavior === 'fail-second' && process.cwd().endsWith('concurrency-2')) ||
      (behavior === 'fail-sweep-second' && process.cwd().includes('/001-bad/'))) process.exitCode = 7;
}
main().catch(e => { console.error(e); process.exitCode = 9; });
"##);
    std::fs::set_permissions(
        project.0.join(agent),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    project
}

#[cfg(unix)]
fn replay(project: &Project, behavior: &str, additional: &[&str]) -> Output {
    let mut args = vec![
        "run",
        "benchmark.toml",
        "--replay",
        "--cassette",
        "tape.jsonl",
    ];
    args.extend_from_slice(additional);
    project.command_with_env(&args, &[("ASB_FIXTURE_BEHAVIOR", behavior)])
}

#[cfg(unix)]
fn assert_tape_scratch_cleaned(project: &Project) {
    let scratch = std::fs::read_dir(&project.0)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".asb-tape-")
        })
        .count();
    assert_eq!(scratch, 0, "tape scratch directories must be removed");
}

#[cfg(unix)]
#[test]
fn replay_returns_json_and_writes_completed_log_offline() {
    let project = replay_project(1);
    let result = replay(&project, "success", &["--json"]);
    expect_success(&result);
    let summary: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(summary["success_rate"], 1.0);
    let log: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.0.join("run.log")).unwrap()).unwrap();
    assert_eq!(log["status"], "completed");
    assert!(
        log["stdout"]
            .as_str()
            .unwrap()
            .contains("fixture-agent-output")
    );
    assert_tape_scratch_cleaned(&project);
}

#[cfg(unix)]
#[test]
fn show_output_streams_mock_agent_output_on_replay() {
    let project = replay_project(1);
    let result = replay(&project, "success", &["--show-output"]);
    expect_success(&result);
    assert!(stdout(&result).contains("fixture-agent-output"));
    assert!(stdout(&result).contains("success_rate=1.000"));
}

#[cfg(unix)]
#[test]
fn replay_rejects_changed_and_extra_requests_and_cleans_up() {
    for behavior in ["mismatch", "extra"] {
        let project = replay_project(1);
        let result = replay(&project, behavior, &[]);
        expect_failure(&result, "replay rejected");
        assert_tape_scratch_cleaned(&project);
    }
}

#[cfg(unix)]
#[test]
fn replay_rejects_partially_consumed_cassettes() {
    let project = replay_project(1);
    let result = replay(&project, "skip", &[]);
    expect_failure(&result, "replay consumed 0 of 1");
    assert_tape_scratch_cleaned(&project);
}

#[cfg(unix)]
#[test]
fn replay_reports_agent_exit_and_zero_exit_fatal_output() {
    for behavior in ["exit", "fatal"] {
        let project = replay_project(1);
        let result = replay(&project, behavior, &[]);
        expect_failure(&result, "agent exited unsuccessfully");
        let log: serde_json::Value =
            serde_json::from_slice(&std::fs::read(project.0.join("run.log")).unwrap()).unwrap();
        assert_eq!(log["status"], "failed");
        assert_tape_scratch_cleaned(&project);
    }
}

#[cfg(unix)]
#[test]
fn concurrent_replay_isolates_replica_outputs_and_reports_partial_failure() {
    let project = replay_project(2);
    let result = replay(&project, "fail-second", &[]);
    expect_failure(&result, "1 concurrent benchmark replica(s) failed");
    for (index, status) in [(1, "completed"), (2, "failed")] {
        let file = project.0.join(format!("run-concurrency-{index}.log"));
        let log: serde_json::Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(log["status"], status);
        assert!(
            project
                .0
                .join(format!("workspace-concurrency-{index}"))
                .is_dir()
        );
    }
    assert_tape_scratch_cleaned(&project);
}

#[cfg(unix)]
#[test]
fn partial_replay_sweep_preserves_both_results_and_writes_report() {
    let project = replay_project(1);
    let mut config = std::fs::read_to_string(project.0.join("benchmark.toml")).unwrap();
    config.push_str("[measurements]\nenabled=true\ndirectory='measurements'\ncollectors=[]\n");
    project.write("good.toml", &config);
    project.write("bad.toml", &config);
    project.write("suite.toml", "configs=['good.toml','bad.toml']\n");
    let tape = std::fs::read_to_string(project.0.join("tape.jsonl")).unwrap();
    for id in ["000-good", "001-bad"] {
        project.write(
            &format!("tapes/{id}/cassettes/benchmark-pi-bug-fix-test-model.jsonl"),
            &tape,
        );
    }
    let result = project.command_with_env(
        &[
            "sweep",
            "suite.toml",
            "--replay",
            "--cassette-root",
            "tapes",
        ],
        &[("ASB_FIXTURE_BEHAVIOR", "fail-sweep-second")],
    );
    expect_failure(&result, "1 failed benchmark(s)");
    for (id, status) in [("000-good", "completed"), ("001-bad", "failed")] {
        let log = project.0.join(format!("suite-results/{id}/run.log"));
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(log).unwrap()).unwrap();
        assert_eq!(value["status"], status);
    }
    assert!(
        project
            .0
            .join("results/suite-comparison/comparison.json")
            .is_file()
    );
    assert!(
        project
            .0
            .join("results/suite-comparison/report.html")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn replay_must_not_expose_a_provider_credential_to_the_agent() {
    use std::os::unix::fs::PermissionsExt;
    let project = replay_project(1);
    project.write("benchmark.toml", "agent='pi'\nworkload='bug-fix'\nprovider='openai'\nmodel='test-model'\nworkspace='workspace'\n");
    let agent = ".asb-tools/pi/bin/pi";
    project.write(agent, r##"#!/usr/bin/env node
const fs=require('fs');
const keys = ['ANTHROPIC_API_KEY', 'ANTHROPIC_AUTH_TOKEN', 'CODEBUDDY_API_KEY', 'CODEBUDDY_AUTH_TOKEN', 'LLM_API_KEY', 'CLINE_API_KEY', 'CONTINUE_API_KEY', 'OPENJIUWEN_API_KEY'];
fs.writeFileSync('credential-observation', process.env.OPENAI_API_KEY === 'asb-replay' && keys.every(key => process.env[key] === undefined) ? 'not-exposed' : 'exposed');
const body={model:'test-model',input:'offline',tools:[{type:'function'}]};
if (!/^http:\/\/127\.0\.0\.1:\d+\/v1$/.test(process.env.ASB_PROVIDER_BASE_URL || '')) process.exit(12);
fetch(process.env.ASB_PROVIDER_BASE_URL + '/responses', {method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)})
  .then(r => { if (!r.ok) process.exitCode=1; })
  .catch(() => { process.exitCode=1; });
"##);
    std::fs::set_permissions(
        project.0.join(agent),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let result = project.command_with_env(
        &[
            "run",
            "benchmark.toml",
            "--replay",
            "--cassette",
            "tape.jsonl",
        ],
        &[
            ("OPENAI_API_KEY", "fixture-secret"),
            ("ANTHROPIC_API_KEY", "another-fixture-secret"),
            ("ANTHROPIC_AUTH_TOKEN", "fixture-auth"),
            ("CODEBUDDY_API_KEY", "fixture-auth"),
            ("CODEBUDDY_AUTH_TOKEN", "fixture-auth"),
            ("LLM_API_KEY", "fixture-auth"),
            ("CLINE_API_KEY", "fixture-auth"),
            ("CONTINUE_API_KEY", "fixture-auth"),
            ("OPENJIUWEN_API_KEY", "fixture-auth"),
        ],
    );
    expect_success(&result);
    assert_eq!(
        std::fs::read_to_string(project.0.join("workspace/credential-observation")).unwrap(),
        "not-exposed"
    );
}

#[cfg(unix)]
#[test]
fn reset_must_not_delete_an_external_directory_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let project = Project::new();
    let project_root = project.0.join("project");
    let outside = project.0.join("outside");
    std::fs::create_dir_all(outside.join("victim")).unwrap();
    std::fs::write(outside.join("victim/sentinel"), "must survive").unwrap();
    expect_success(&project.command(&["init", project_root.to_str().unwrap()]));
    symlink(&outside, project_root.join("link")).unwrap();
    std::fs::write(
        project_root.join(".asb/project.toml"),
        "tools_dir='link/victim'\nworkloads_dir='workloads'\nworkspace_dir='asb-workspace'\n",
    )
    .unwrap();
    let result = project.command(&["init", project_root.to_str().unwrap(), "--reset"]);
    expect_failure(&result, "reset requires a direct child");
    assert!(project_root.join(".asb/project.toml").is_file());
    assert!(
        outside.join("victim/sentinel").is_file(),
        "reset deleted data outside the project"
    );
}

#[cfg(unix)]
#[test]
fn reset_rejects_symlinked_default_directories_before_changing_state() {
    use std::os::unix::fs::symlink;
    let project = Project::new();
    let root = project.0.join("project");
    let outside = project.0.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    expect_success(&project.command(&["init", root.to_str().unwrap()]));
    // Only remove this newly created empty fixture directory.
    std::fs::remove_dir(root.join(".asb-tools")).unwrap();
    symlink(&outside, root.join(".asb-tools")).unwrap();
    let result = project.command(&["init", root.to_str().unwrap(), "--reset"]);
    expect_failure(&result, "is a symbolic link");
    assert!(root.join(".asb/project.toml").is_file());
    assert!(outside.is_dir());
}

#[cfg(target_os = "linux")]
fn install_fake_bridge(project: &Project, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    let name = ".asb-tools/cli2key/node_modules/.bin/codex-bridge";
    project.write(name, source);
    std::fs::set_permissions(project.0.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(target_os = "linux")]
struct WaitForFixtureExpiry {
    pid: u32,
    executable: PathBuf,
}

#[cfg(target_os = "linux")]
impl Drop for WaitForFixtureExpiry {
    fn drop(&mut self) {
        use std::os::unix::ffi::OsStrExt;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            let cmdline = std::fs::read(format!("/proc/{}/cmdline", self.pid)).unwrap_or_default();
            if !cmdline
                .split(|byte| *byte == 0)
                .any(|arg| arg == self.executable.as_os_str().as_bytes())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[cfg(target_os = "linux")]
fn wait_for_fixture(project: &Project, pid: u32) -> WaitForFixtureExpiry {
    WaitForFixtureExpiry {
        pid,
        executable: project
            .0
            .join(".asb-tools/cli2key/node_modules/.bin/codex-bridge"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn provider_models_fetches_all_authenticated_catalog_pages() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    for (provider, env_key, env_url, first, second, expected_query, expected_header) in [
        (
            "anthropic",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_BASE_URL",
            r#"{"data":[{"id":"first"}],"has_more":true,"last_id":"a/b"}"#,
            r#"{"data":[{"id":"second"}],"has_more":false}"#,
            "after_id=a%2Fb",
            "x-api-key: fixture-key",
        ),
        (
            "gemini",
            "GEMINI_API_KEY",
            "GOOGLE_GEMINI_BASE_URL",
            r#"{"models":[{"name":"models/first"}],"nextPageToken":"a/b"}"#,
            r#"{"models":[{"name":"models/second"}]}"#,
            "pageToken=a%2Fb",
            "x-goog-api-key: fixture-key",
        ),
        (
            "fireworks",
            "FIREWORKS_API_KEY",
            "FIREWORKS_MODELS_URL",
            r#"{"models":[{"name":"first","supportsTools":true}],"nextPageToken":"a/b"}"#,
            r#"{"models":[{"name":"second","supportsTools":true}]}"#,
            "pageToken=a%2Fb",
            "authorization: bearer fixture-key",
        ),
    ] {
        let project = Project::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1beta", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut requests = Vec::new();
            while requests.len() < 2 && Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("fixture accept: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut buf = [0; 8192];
                let len = stream.read(&mut buf).unwrap();
                requests.push(String::from_utf8_lossy(&buf[..len]).to_string());
                let body = if requests.len() == 1 { first } else { second };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
            requests
        });
        let result = project.command_with_env(
            &["provider", "models", provider, "--json"],
            &[(env_key, "fixture-key"), (env_url, &base)],
        );
        let requests = server.join().unwrap();
        expect_success(&result);
        assert_eq!(requests.len(), 2);
        assert!(requests[1].lines().next().unwrap().contains(expected_query));
        assert!(
            requests
                .iter()
                .all(|request| request.to_ascii_lowercase().contains(expected_header))
        );
        let data: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(data["models"], serde_json::json!(["first", "second"]));
    }
}

#[test]
fn together_provider_models_accepts_documented_array_response() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(value) => break value,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 8192];
        let len = stream.read(&mut request).unwrap();
        let body = r#"[{"id":"example/chat","type":"chat"}]"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
        String::from_utf8_lossy(&request[..len]).into_owned()
    });
    let project = Project::new();
    let result = project.command_with_env(
        &["provider", "models", "together", "--json"],
        &[
            ("TOGETHER_API_KEY", "fixture-key"),
            ("TOGETHER_BASE_URL", &base),
        ],
    );
    let request = server.join().unwrap();
    expect_success(&result);
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-key")
    );
    let data: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(data["models"], serde_json::json!(["example/chat"]));
}

#[cfg(target_os = "linux")]
#[test]
fn cli2key_run_validates_current_bridge_catalog_and_reaps_bridge() {
    use std::os::unix::fs::PermissionsExt;
    for (model, expect_ok) in [("current-model", true), ("removed-model", false)] {
        let project = Project::new();
        project.init();
        project.write("benchmark.toml", &format!("agent='pi'\nworkload='bug-fix'\nprovider='cli2key'\nmodel='{model}'\nworkspace='workspace'\nlog_file='run.log'\n"));
        project.write(
            ".asb-tools/pi/bin/pi",
            "#!/bin/sh\nprintf 'fixture agent complete\\n'\n",
        );
        std::fs::set_permissions(
            project.0.join(".asb-tools/pi/bin/pi"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        install_fake_bridge(
            &project,
            r##"#!/usr/bin/env node
const http = require('http'), fs = require('fs');
fs.writeFileSync(process.env.ASB_TEST_PIDFILE, String(process.pid));
http.createServer((req,res) => {
  fs.writeFileSync(process.env.ASB_TEST_OBSERVED, req.url + ' ' + req.headers.authorization);
  res.writeHead(200, {'content-type':'application/json'});
  res.end(JSON.stringify({data:[{id:'current-model'}]}));
}).listen(Number(process.env.CODEX_BRIDGE_PORT), '127.0.0.1', () => console.log('Codex Bridge API key: fixture-key'));
"##,
        );
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
            .to_string();
        let pidfile = project.0.join("bridge.pid");
        let observed = project.0.join("observed");
        let result = project.command_with_env(
            &["run", "benchmark.toml"],
            &[
                ("CODEX_BRIDGE_PORT", &port),
                ("ASB_TEST_PIDFILE", pidfile.to_str().unwrap()),
                ("ASB_TEST_OBSERVED", observed.to_str().unwrap()),
            ],
        );
        let pid: u32 = std::fs::read_to_string(pidfile).unwrap().parse().unwrap();
        let _wait = wait_for_fixture(&project, pid);
        if expect_ok {
            expect_success(&result);
        } else {
            expect_failure(&result, "was not returned by provider");
        }
        assert_eq!(
            std::fs::read_to_string(observed).unwrap(),
            "/v1/models Bearer fixture-key"
        );
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "bridge was not reaped"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cli2key_success_stops_its_mock_bridge_after_model_discovery() {
    let project = Project::new();
    project.init();
    install_fake_bridge(
        &project,
        r##"#!/usr/bin/env node
const http = require('http'), fs = require('fs');
fs.writeFileSync(process.env.ASB_TEST_PIDFILE, String(process.pid));
// Mark timeout separately so a slow test cannot mistake it for ASB cleanup.
setTimeout(() => { fs.writeFileSync(process.env.ASB_TEST_EXPIRED, 'expired'); process.exit(0); }, 2000).unref();
http.createServer((req,res) => {
  if (req.url !== '/v1/models') { res.writeHead(404); return res.end(); }
  res.writeHead(200, {'content-type':'application/json'});
  res.end(JSON.stringify({data:[{id:'fixture-model'}]}));
}).listen(Number(process.env.CODEX_BRIDGE_PORT), '127.0.0.1', () => {
  console.log('Codex Bridge API key: fixture-key');
});
"##,
    );
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
        .to_string();
    let pidfile = project.0.join("bridge.pid");
    let expired = project.0.join("expired");
    let result = project.command_with_env(
        &["provider", "models", "cli2key"],
        &[
            ("CODEX_BRIDGE_PORT", &port),
            ("ASB_TEST_PIDFILE", pidfile.to_str().unwrap()),
            ("ASB_TEST_EXPIRED", expired.to_str().unwrap()),
        ],
    );
    let pid: u32 = std::fs::read_to_string(pidfile).unwrap().parse().unwrap();
    let _wait = wait_for_fixture(&project, pid);
    expect_success(&result);
    assert!(!expired.exists(), "mock timed out before ASB completed");
    assert!(stdout(&result).contains("fixture-model"));
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "bridge was not reaped"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn cli2key_failed_startup_must_stop_its_mock_bridge() {
    let project = Project::new();
    project.init();
    install_fake_bridge(
        &project,
        "#!/usr/bin/env node\nconst fs=require('fs'); fs.writeFileSync(process.env.ASB_TEST_PIDFILE, String(process.pid)); fs.closeSync(1); setTimeout(() => fs.writeFileSync(process.env.ASB_TEST_EXPIRED, 'expired'), 2000);\n",
    );
    let pidfile = project.0.join("bridge.pid");
    let expired = project.0.join("expired");
    let result = project.command_with_env(
        &["provider", "models", "cli2key"],
        &[
            ("ASB_TEST_PIDFILE", pidfile.to_str().unwrap()),
            ("ASB_TEST_EXPIRED", expired.to_str().unwrap()),
        ],
    );
    let pid: u32 = std::fs::read_to_string(pidfile).unwrap().parse().unwrap();
    let _wait = wait_for_fixture(&project, pid);
    expect_failure(&result, "closed its startup output");
    assert!(!expired.exists(), "mock timed out before ASB completed");
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "bridge survived failed startup"
    );
}
