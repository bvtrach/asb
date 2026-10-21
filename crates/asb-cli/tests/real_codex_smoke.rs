//! Opt-in real-agent smoke against a loopback Responses fixture, never a live provider.
#![cfg(unix)]

mod support;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::symlink;
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use support::IsolatedEnvironment;

#[test]
#[ignore = "requires ASB_REAL_CODEX pointing to a local Codex executable"]
fn real_codex_uses_selected_openrouter_responses_endpoint() {
    let executable = std::env::var_os("ASB_REAL_CODEX").expect("set ASB_REAL_CODEX");
    exercise_agent("codex", "codex", executable, false);
}

#[test]
#[ignore = "requires ASB_REAL_AGENT_ID, ASB_REAL_AGENT_BIN and ASB_REAL_AGENT_EXECUTABLE"]
fn real_agent_uses_selected_openrouter_endpoint() {
    let id = std::env::var("ASB_REAL_AGENT_ID").expect("set ASB_REAL_AGENT_ID");
    let binary = std::env::var("ASB_REAL_AGENT_BIN").expect("set ASB_REAL_AGENT_BIN");
    let executable =
        std::env::var_os("ASB_REAL_AGENT_EXECUTABLE").expect("set ASB_REAL_AGENT_EXECUTABLE");
    exercise_agent(&id, &binary, executable, false);
}

#[test]
#[ignore = "requires an installed ASB_REAL_AGENT_ID, ASB_REAL_AGENT_BIN and ASB_REAL_AGENT_EXECUTABLE"]
fn real_agent_reports_provider_usage_from_loopback_fixture() {
    let id = std::env::var("ASB_REAL_AGENT_ID").expect("set ASB_REAL_AGENT_ID");
    assert!(["continue", "openhands"].contains(&id.as_str()));
    let binary = std::env::var("ASB_REAL_AGENT_BIN").expect("set ASB_REAL_AGENT_BIN");
    let executable =
        std::env::var_os("ASB_REAL_AGENT_EXECUTABLE").expect("set ASB_REAL_AGENT_EXECUTABLE");
    exercise_agent(&id, &binary, executable, true);
}

fn exercise_agent(agent: &str, binary: &str, executable: std::ffi::OsString, usage_only: bool) {
    // Real agents may ignore a configured base URL. Refuse to launch them
    // unless all network routes are confined to this namespace's loopback.
    for path in ["/proc/net/route", "/proc/net/ipv6_route"] {
        let routes = std::fs::read_to_string(path).expect("inspect network namespace routes");
        for line in routes.lines().skip(usize::from(path.ends_with("/route"))) {
            let interface = if path.ends_with("ipv6_route") {
                line.split_whitespace().last()
            } else {
                line.split_whitespace().next()
            };
            assert!(
                interface.is_none_or(|name| name == "lo"),
                "real-agent smoke requires loopback-only network namespace; found route on {interface:?}"
            );
        }
    }
    let provider = std::env::var("ASB_REAL_PROVIDER").unwrap_or_else(|_| "openrouter".into());
    let split_cache = std::env::var_os("ASB_TEST_SPLIT_CACHE").is_some();
    let model = std::env::var("ASB_REAL_MODEL").unwrap_or_else(|_| "fixture/model".into());
    let deadline = std::env::var("ASB_REAL_TIMEOUT").unwrap_or_else(|_| "25s".into());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("real-codex-smoke-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let worker_stop = Arc::clone(&stop);
    let worker_seen = Arc::clone(&seen);
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fixture accept: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            let (key, value) = line.split_once(':')?;
                            key.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request).into_owned();
            let line = request.lines().next().unwrap_or_default().to_owned();
            let streaming = request
                .split_once("\r\n\r\n")
                .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(body).ok())
                .and_then(|body| body.get("stream").and_then(serde_json::Value::as_bool))
                .unwrap_or(false);
            let reply = if request.contains("What color is this image?") {
                "red"
            } else {
                "src/cli.py src/registry.py lookup"
            };
            worker_seen.lock().unwrap().push(request);
            let (content_type, body) = if line.starts_with("GET /catalog ") {
                ("application/json", serde_json::json!({"opencode":{"api":"https://opencode.ai/inference/openai/v1","npm":"@ai-sdk/openai-compatible","models":{"fixture/model":{"provider":{"npm":"@ai-sdk/openai","api":"https://opencode.ai/inference/openai/v1"}}}}}).to_string())
            } else if line.starts_with("GET /v1/models ") {
                (
                    "application/json",
                    r#"{"data":[{"id":"fixture/model","supported_parameters":["tools"]},{"id":"gpt-6-astra","supported_parameters":["tools"]}]}"#
                        .to_owned(),
                )
            } else if line.starts_with("POST /v1/responses ") {
                let events = [
                    serde_json::json!({"type":"response.created","sequence_number":0,"response":{"id":"resp_fixture","object":"response","created_at":1,"status":"in_progress","model":"fixture/model","output":[],"error":null,"incomplete_details":null}}),
                    serde_json::json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":{"id":"msg_fixture","type":"message","status":"in_progress","role":"assistant","content":[]}}),
                    serde_json::json!({"type":"response.content_part.added","sequence_number":2,"item_id":"msg_fixture","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
                    serde_json::json!({"type":"response.output_text.delta","sequence_number":3,"item_id":"msg_fixture","output_index":0,"content_index":0,"delta":reply}),
                    serde_json::json!({"type":"response.output_text.done","sequence_number":4,"item_id":"msg_fixture","output_index":0,"content_index":0,"text":reply}),
                    serde_json::json!({"type":"response.content_part.done","sequence_number":5,"item_id":"msg_fixture","output_index":0,"content_index":0,"part":{"type":"output_text","text":reply,"annotations":[]}}),
                    serde_json::json!({"type":"response.output_item.done","sequence_number":6,"output_index":0,"item":{"id":"msg_fixture","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":reply,"annotations":[]}]}}),
                    serde_json::json!({"type":"response.completed","sequence_number":7,"response":{"id":"resp_fixture","object":"response","created_at":1,"status":"completed","model":"fixture/model","output":[{"id":"msg_fixture","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":reply,"annotations":[]}]}],"error":null,"incomplete_details":null,"usage":{"input_tokens":10,"input_tokens_details":{"cached_tokens":0},"output_tokens":3,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":13}}}),
                ];
                (
                    "text/event-stream",
                    events
                        .iter()
                        .map(|event| {
                            format!(
                                "event: {}\ndata: {event}\n\n",
                                event["type"].as_str().unwrap()
                            )
                        })
                        .collect::<String>(),
                )
            } else if line.starts_with("POST /v1/chat/completions ") {
                if !streaming {
                    ("application/json", serde_json::json!({"id":"chatcmpl_fixture","object":"chat.completion","created":1,"model":"fixture/model","choices":[{"index":0,"message":{"role":"assistant","content":reply},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13,"prompt_tokens_details":{"cached_tokens":if split_cache {4} else {0}}}}).to_string())
                } else {
                    ("text/event-stream", [
                    serde_json::json!({"id":"chatcmpl_fixture","object":"chat.completion.chunk","created":1,"model":"fixture/model","choices":[{"index":0,"delta":{"role":"assistant","content":reply},"finish_reason":null}]}),
                    serde_json::json!({"id":"chatcmpl_fixture","object":"chat.completion.chunk","created":1,"model":"fixture/model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}),
                    serde_json::json!({"id":"chatcmpl_fixture","object":"chat.completion.chunk","created":1,"model":"fixture/model","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13,"prompt_tokens_details":{"cached_tokens":if split_cache {4} else {0}}}}),
                ].iter().map(|event| format!("data: {event}\n\n")).collect::<String>() + "data: [DONE]\n\n")
                }
            } else if line.starts_with("POST /v1/messages ")
                || line.starts_with("POST /v1/messages?")
            {
                if !streaming {
                    ("application/json", serde_json::json!({"id":"msg_fixture","type":"message","role":"assistant","model":"fixture/model","content":[{"type":"text","text":reply}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":3,"cache_read_input_tokens":if split_cache {4} else {0},"cache_creation_input_tokens":if split_cache {2} else {0}}}).to_string())
                } else {
                    let events = [
                        serde_json::json!({"type":"message_start","message":{"id":"msg_fixture","type":"message","role":"assistant","model":"fixture/model","content":[],"stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":if split_cache {4} else {0},"cache_creation_input_tokens":if split_cache {2} else {0}}}}),
                        serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                        serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":reply}}),
                        serde_json::json!({"type":"content_block_stop","index":0}),
                        serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":3}}),
                        serde_json::json!({"type":"message_stop"}),
                    ];
                    (
                        "text/event-stream",
                        events
                            .iter()
                            .map(|event| {
                                format!(
                                    "event: {}\ndata: {event}\n\n",
                                    event["type"].as_str().unwrap()
                                )
                            })
                            .collect(),
                    )
                }
            } else {
                ("application/json", "{}".to_owned())
            };
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    let run = |args: &[&str]| {
        Command::new("timeout")
            .current_dir(&root)
            .args(["--kill-after=2s", &deadline, env!("CARGO_BIN_EXE_asb")])
            .args(args)
            .isolated_env()
            .env("HOME", &root)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("PYTHONUNBUFFERED", "1")
            .env("LITELLM_LOCAL_MODEL_COST_MAP", "True")
            .env("OPENROUTER_API_KEY", "fixture-key")
            .env("OPENROUTER_BASE_URL", format!("http://{address}/v1"))
            .env("ANTHROPIC_API_KEY", "fixture-key")
            .env("ANTHROPIC_BASE_URL", format!("http://{address}/v1"))
            .env("CLI2KEY_API_KEY", "fixture-key")
            .env("CLI2KEY_BASE_URL", format!("http://{address}/v1"))
            .env("DEEPSEEK_API_KEY", "fixture-key")
            .env("DEEPSEEK_BASE_URL", format!("http://{address}/v1"))
            .env(
                "DEEPSEEK_ANTHROPIC_BASE_URL",
                format!("http://{address}/v1"),
            )
            .env("OPENCODE_API_KEY", "fixture-key")
            .env("OPENCODE_ZEN_BASE_URL", format!("http://{address}/v1"))
            .env(
                "ASB_ENDPOINT_CATALOG_URL",
                format!("http://{address}/catalog"),
            )
            .output()
            .unwrap()
    };
    let init = run(&["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let bin_dir = root.join(".asb-tools").join(agent).join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    symlink(executable, bin_dir.join(binary)).unwrap();
    std::fs::write(root.join("benchmark.toml"), format!("agent='{agent}'\nworkload='repository-navigation'\nprovider='{provider}'\nmodel='{model}'\nworkspace='workspace'\nlog_file='run.log'\n")).unwrap();
    let output = run(&["run", "benchmark.toml", "--show-output"]);
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert!(
        output.status.success()
            || (usage_only
                && String::from_utf8_lossy(&output.stderr).contains("workload check failed:")),
        "agent={agent} stdout={} stderr={} requests={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
        seen.lock()
            .unwrap()
            .iter()
            .map(|s| (
                s.lines().next().unwrap_or(""),
                s.lines()
                    .filter(
                        |line| line.to_ascii_lowercase().starts_with("content-length:")
                            || line.to_ascii_lowercase().starts_with("transfer-encoding:")
                    )
                    .collect::<Vec<_>>(),
                s.split_once("\r\n\r\n").map(|(_, body)| (
                    body.len(),
                    body.contains("\"stream\": true"),
                    body.contains("\"stream\":true"),
                    body.chars().take(700).collect::<String>()
                ))
            ))
            .collect::<Vec<_>>()
    );
    let seen = seen.lock().unwrap();
    if provider == "cli2key" {
        assert!(
            seen.iter()
                .any(|request| request.starts_with("GET /v1/models ")
                    && request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer fixture-key")),
            "the bridge's live model catalog was not queried with the bridge credential"
        );
    }
    if provider == "opencode-zen" {
        assert!(
            seen.iter()
                .any(|request| request.starts_with("GET /catalog ")),
            "endpoint catalog was not refreshed"
        );
    }
    let responses = seen
        .iter()
        .filter(|request| {
            request.starts_with("POST /v1/responses ")
                || request.starts_with("POST /v1/chat/completions ")
                || request.starts_with("POST /v1/messages ")
                || request.starts_with("POST /v1/messages?")
        })
        .collect::<Vec<_>>();
    assert!(
        !responses.is_empty(),
        "real agent did not reach the fixture"
    );
    assert!(
        responses
            .iter()
            .any(|request| !request.contains("What color is this image?")),
        "the only model request was OpenJiuwen's image-capability probe, not the benchmark task"
    );
    if matches!(agent, "qwen-code" | "openjiuwen") || usage_only {
        let run: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("run.log")).unwrap()).unwrap();
        let usage = &run["token_usage"];
        let calls = responses
            .iter()
            .filter(|request| !request.contains("What color is this image?"))
            .count() as u64;
        let input = if split_cache && provider == "anthropic" {
            16
        } else {
            10
        };
        assert_eq!(
            usage["input_tokens"],
            calls * input,
            "{agent} usage: {usage}"
        );
        assert_eq!(usage["output_tokens"], calls * 3, "{agent} usage: {usage}");
        if split_cache && usage_only {
            assert_eq!(
                usage["cached_tokens"],
                calls * 4,
                "{agent} cached usage: {usage}"
            );
        }
        assert_eq!(
            usage["total_tokens"],
            calls * (input + 3),
            "{agent} usage: {usage}"
        );
    }
    let headers = responses[0].to_ascii_lowercase();
    assert!(
        headers.contains("authorization: bearer fixture-key")
            || headers.contains("x-api-key: fixture-key"),
        "fixture credential was not sent to selected endpoint"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
