//! Transport/capability fixtures never load a user's key files or contact APIs.

#[test]
fn metadata_and_authenticated_configuration_never_echo_untrusted_values() {
    let metadata = CurlMetadata::parse(
        b"ASB_HTTP_STATUS=99\nASB_PROXY_STATUS=999\nASB_CONTENT_TYPE=unknown/fixture-secret\n",
    );
    assert_eq!(metadata.http_status, None);
    assert_eq!(metadata.proxy_status, None);
    assert_eq!(metadata.content_type.as_deref(), Some("other"));
    for (mime, expected) in [
        ("Application/JSON; charset=utf-8", "application/json"),
        ("text/plain", "text/plain"),
        ("application/octet-stream", "application/octet-stream"),
        ("", ""),
    ] {
        let metadata = CurlMetadata::parse(
            format!("ASB_HTTP_STATUS=201\nASB_PROXY_STATUS=200\nASB_CONTENT_TYPE={mime}\n")
                .as_bytes(),
        );
        assert_eq!(metadata.http_status, Some(201));
        assert_eq!(metadata.proxy_status, Some(200));
        if expected.is_empty() {
            assert_eq!(metadata.content_type, None);
        } else {
            assert_eq!(metadata.content_type.as_deref(), Some(expected));
        }
    }
    let spec = providers::model_spec("openai").unwrap();
    for (url, key) in [
        ("http://fixture.invalid\nhidden", "dummy"),
        ("http://fixture.invalid", "dummy\rhidden"),
    ] {
        let error = fetch_authenticated(spec, url, key).unwrap_err().to_string();
        assert!(error.contains("newline"));
        assert!(!error.contains("hidden"));
    }
    let error = curl_config(
        spec,
        "http://fixture.invalid",
        Some("fixture-secret\nheader"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("newline"));
    assert!(!error.contains("fixture-secret"));
    assert!(
        parse_models(
            br#"{"data":[{"id":"all"}]}"#,
            providers::ModelKind::DataUnpaginated,
            "openai",
            false
        )
        .is_err()
    );
}

use super::*;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
#[cfg(unix)]
#[test]
fn isolated_curl_transport_errors_are_bounded_actionable_and_redacted() {
    use std::os::unix::fs::PermissionsExt;
    const CASE: &str = "ASB_MODEL_FETCH_CASE";
    const SECRET: &str = "fixture-only-private-key";
    let Ok(case) = std::env::var(CASE) else {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/model-fetch-fixtures")
            .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let python = crate::tools::host_executable(Path::new("python3")).unwrap();
        let name = format!(
            "{}::isolated_curl_transport_errors_are_bounded_actionable_and_redacted",
            module_path!().split_once("::").unwrap().1
        );
        for case in [
            "ok",
            "proxy407",
            "http301",
            "http401",
            "http403",
            "http407",
            "http404",
            "http429",
            "http503",
            "exit5",
            "exit6",
            "exit7",
            "exit28",
            "exit35",
            "exit51",
            "exit60",
            "exit77",
            "exit90",
            "exit56",
            "exit22",
            "exit63",
            "signal",
            "truncated",
            "html",
            "unknown-type",
            "oversize",
            "missing-curl",
            "nonexec-curl",
            "broken-input",
            "capabilities",
            "list",
        ] {
            let directory = root.join(case);
            fs::create_dir_all(directory.join("commands")).unwrap();
            fs::create_dir_all(directory.join("home")).unwrap();
            fs::create_dir_all(directory.join(".asb")).unwrap();
            fs::write(
                directory.join(".asb/project.toml"),
                toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
            )
            .unwrap();
            let curl = directory.join("commands/curl");
            let program = format!(
                r#"#!{}
import json,os,signal,sys
from pathlib import Path
case=os.environ['ASB_MODEL_FETCH_CASE']
root=Path(os.environ['HOME']).parent
if case=='broken-input':
 os.close(0)
 sys.exit(2)
configuration=sys.stdin.read()
(root/'stdin').write_text(configuration)
(root/'arguments').write_text(json.dumps(sys.argv[1:]))
assert '--config' in sys.argv and sys.argv[sys.argv.index('--config')+1]=='-'
assert '--location' not in sys.argv
body={{'data':[{{'id':'supported','tool_calling':True}},{{'id':'unsupported','tool_calling':False}},{{'id':'unknown'}}]}}
http=200
proxy=0
mime='application/json'
if case=='proxy407': proxy=407
if case.startswith('http'): http=int(case[4:])
if case=='html': mime='text/html'; body='<html>fixture-only-private-key</html>'
if case=='truncated': body='{{"data":['
if case=='unknown-type': body='not-json'; mime='sensitive-custom-mime-fixture-only-private-key'
if case=='oversize': body='x'*(16*1024*1024+10)
sys.stderr.write('fixture-only-private-key secret proxy diagnostic\n'*300)
sys.stderr.write('\nASB_HTTP_STATUS='+str(http)+'\nASB_PROXY_STATUS='+str(proxy)+'\nASB_CONTENT_TYPE='+mime+'\n')
sys.stderr.flush()
sys.stdout.write(body if isinstance(body,str) else json.dumps(body))
sys.stdout.flush()
if case=='signal': os.kill(os.getpid(),signal.SIGTERM)
sys.exit(int(case[4:]) if case.startswith('exit') else 0)
"#,
                python.display()
            );
            if case != "missing-curl" {
                fs::write(&curl, program).unwrap();
                fs::set_permissions(
                    &curl,
                    fs::Permissions::from_mode(if case == "nonexec-curl" { 0o600 } else { 0o700 }),
                )
                .unwrap();
            }
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .current_dir(&directory)
                .env_clear()
                .env(CASE, case)
                .env("PATH", directory.join("commands"))
                .env("HOME", directory.join("home"));
            if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
                command.env("LLVM_PROFILE_FILE", profile);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        }
        fs::remove_dir_all(root).unwrap();
        return;
    };
    assert!(std::env::var_os("OPENAI_API_KEY").is_none());
    let command_root = PathBuf::from(std::env::var_os("PATH").unwrap());
    assert_eq!(command_root.file_name().unwrap(), "commands");
    if case == "capabilities" {
        let bridge = Some(("http://fixture.invalid/v1", SECRET));
        assert!(ensure_tool_capable("openrouter", "supported", bridge).unwrap());
        assert!(!ensure_tool_capable("openrouter", "unknown", bridge).unwrap());
        assert!(
            ensure_tool_capable("openrouter", "unsupported", bridge)
                .unwrap_err()
                .to_string()
                .contains("does not support")
        );
        assert!(
            ensure_tool_capable("openrouter", "missing", bridge)
                .unwrap_err()
                .to_string()
                .contains("was not returned")
        );
        assert!(ensure_tool_capable("not-a-provider", "missing", bridge).is_err());
        return;
    }
    if case == "list" {
        let mut text = Vec::new();
        list(
            ModelListOptions {
                provider: Some("openrouter".into()),
                json: false,
                free_only: false,
            },
            &mut text,
        )
        .unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("openrouter models (2):"));
        assert!(!text.contains("unsupported"));
        let mut text = Vec::new();
        list(
            ModelListOptions {
                provider: Some("openrouter".into()),
                json: true,
                free_only: false,
            },
            &mut text,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&text).unwrap();
        assert_eq!(value["models"], json!(["supported", "unknown"]));
        return;
    }
    let configuration = if case == "broken-input" {
        "x".repeat(2 * 1024 * 1024)
    } else {
        format!(
            "url = \"http://fixture.invalid/models\"\nheader = \"Authorization: Bearer {SECRET}\"\n"
        )
    };
    let result = fetch_config(configuration, "fixture-provider");
    if case == "ok" {
        let body = result.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["data"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let root = PathBuf::from(std::env::var_os("HOME").unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        assert!(
            fs::read_to_string(root.join("stdin"))
                .unwrap()
                .contains(SECRET)
        );
        assert!(
            !fs::read_to_string(root.join("arguments"))
                .unwrap()
                .contains(SECRET)
        );
        let arguments: Vec<String> =
            serde_json::from_str(&fs::read_to_string(root.join("arguments")).unwrap()).unwrap();
        for (flag, expected) in [
            ("--max-filesize", transport::MAX_RESPONSE.to_string()),
            ("--max-time", transport::IO_TIMEOUT.as_secs().to_string()),
        ] {
            let index = arguments
                .iter()
                .position(|argument| argument == flag)
                .unwrap();
            assert_eq!(
                arguments[index + 1],
                expected,
                "curl and reader/watchdog must share one limit"
            );
        }
        return;
    }
    let error = result.unwrap_err().to_string();
    assert!(!error.contains(SECRET), "{error}");
    assert!(!error.contains("secret proxy diagnostic"), "{error}");
    let expected = match case.as_str() {
        "proxy407" => "HTTP proxy returned 407",
        "http301" => "Redirects are not followed",
        "http401" | "http403" => "provider API key",
        "http407" => "proxy authentication",
        "http404" => "Check the provider endpoint",
        "http429" => "rate-limited",
        "http503" => "server error",
        "exit5" => "resolve the configured proxy",
        "exit6" => "resolve the provider host",
        "exit7" => "could not connect",
        "exit28" => "timed out",
        "exit35" | "exit51" | "exit60" | "exit77" | "exit90" => "TLS verification",
        "exit56" => "connection was interrupted",
        "exit22" => "provider connection failed",
        "exit63" | "oversize" => "16 MiB response limit",
        "signal" => "curl exit signal",
        "truncated" => "incomplete or truncated JSON",
        "html" => "content type text/html",
        "unknown-type" => "content type other",
        "missing-curl" => "model discovery needs",
        "nonexec-curl" => "cannot start curl",
        "broken-input" => "cannot send model discovery request",
        _ => panic!("unknown fixture case"),
    };
    assert!(error.contains(expected), "{error}");
}
#[test]
fn multi_provider_rendering_and_pagination_boundaries_need_no_transport() {
    let entries = || {
        vec![
            ModelInfo {
                id: "allowed".into(),
                tool_calling: Some(true),
            },
            ModelInfo {
                id: "unknown".into(),
                tool_calling: None,
            },
            ModelInfo {
                id: "rejected".into(),
                tool_calling: Some(false),
            },
        ]
    };
    for json in [false, true] {
        let mut output = Vec::new();
        render_all(
            &mut output,
            vec![
                ("ok-provider".into(), Ok(entries())),
                (
                    "failed-provider".into(),
                    Err(Error::Input("fixture unavailable".into())),
                ),
            ],
            json,
        )
        .unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("allowed"));
        assert!(output.contains("unknown"));
        assert!(!output.contains("rejected"));
        assert!(output.contains("unavailable"));
        if json {
            let value: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(value["failed-provider"]["status"], "unavailable");
        }
    }
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("fixture output unavailable"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for json in [false, true] {
        assert!(render_one(&mut Broken, "fixture", entries(), json).is_err());
        assert!(render_all(&mut Broken, vec![("fixture".into(), Ok(entries()))], json).is_err());
    }
    let mut calls = 0;
    let error = collect_catalog_pages(
        providers::ModelKind::DataCursor,
        "http://fixture.invalid",
        |_| {
            calls += 1;
            Ok(serde_json::to_vec(
                &json!({"data":[],"has_more":true,"last_id":format!("cursor-{calls}")}),
            )
            .unwrap())
        },
    )
    .unwrap_err();
    assert_eq!(calls, 16);
    assert!(error.to_string().contains("exceeds 16 pages"));
    let mut calls = 0;
    let error = collect_catalog_pages(
        providers::ModelKind::DataCursor,
        "http://fixture.invalid",
        |_| {
            calls += 1;
            let mut body = serde_json::to_vec(
                &json!({"data":[],"has_more":true,"last_id":format!("cursor-{calls}")}),
            )
            .unwrap();
            body.resize(2 * 1024 * 1024 + 10, b' ');
            Ok(body)
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("exceeds 32 MiB"));
}
