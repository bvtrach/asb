//! Live model discovery through provider model-list endpoints.

use crate::Error;
use crate::cli2key;
use crate::project;
use crate::providers;
use crate::suggestions;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_CASSETTE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelInfo {
    id: String,
    tool_calling: bool,
}

/// List models for one provider, or all providers when no name is supplied.
pub fn list(args: &[String], output: &mut dyn Write) -> Result<(), Error> {
    let (provider, json) = parse_args(args)?;
    if let Some(provider) = provider {
        let result = discover(&provider)?;
        render_one(output, &provider, result, json)
    } else {
        let mut all = Vec::new();
        for name in providers::names() {
            match discover(name) {
                Ok(models) => all.push(((*name).to_owned(), Ok(models))),
                Err(error) => all.push(((*name).to_owned(), Err(error))),
            }
        }
        render_all(output, all, json)
    }
}

fn parse_args(args: &[String]) -> Result<(Option<String>, bool), Error> {
    let mut provider = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if provider.is_none() && providers::is_valid_name(arg) {
            provider = Some(arg.clone());
        } else {
            return Err(Error::Usage("provider models accepts PROVIDER and --json"));
        }
    }
    Ok((provider, json))
}

fn discover(provider: &str) -> Result<Vec<ModelInfo>, Error> {
    let spec = providers::model_spec(provider).ok_or_else(|| {
        Error::Input(suggestions::unknown(
            "provider",
            provider,
            providers::names(),
        ))
    })?;
    if provider.eq_ignore_ascii_case("cli2key") {
        let bridge = cli2key::Bridge::connect(local_bridge_executable().as_deref())?;
        let url = format!("{}/models", bridge.base_url().trim_end_matches('/'));
        let body = fetch_authenticated(spec, &url, bridge.api_key())?;
        return parse_models(&body, spec.kind, provider);
    }
    if let Some(credential) = spec.credential {
        if providers::credential(provider).is_none() {
            return Err(Error::Input(format!(
                "missing {credential} or ~/.api_key_{}",
                spec.name
            )));
        }
    }
    if spec.url().is_none() {
        let endpoint = spec.endpoint.unwrap_or("provider endpoint variable");
        return Err(Error::Input(format!("missing {endpoint}")));
    }
    let body = fetch(spec)?;
    parse_models(&body, spec.kind, provider)
}

/// Require a model to be advertised as tool-capable before execution.
pub fn ensure_tool_capable(provider: &str, model: &str) -> Result<(), Error> {
    if let Some(models) = providers::known_models(provider) {
        if models.iter().any(|candidate| *candidate == model) {
            return Ok(());
        }
        return Err(Error::Input(format!(
            "model `{model}` for provider `{provider}` is not in the tool-capable catalog; run `asb provider models {provider}` and choose one of the listed models"
        )));
    }
    let models = discover(provider)?;
    match models.into_iter().find(|candidate| candidate.id == model) {
        Some(candidate) if candidate.tool_calling => Ok(()),
        Some(_) => Err(Error::Input(format!(
            "model `{model}` for provider `{provider}` does not advertise tool calling; choose a model listed by `asb provider models {provider}`"
        ))),
        None => Err(Error::Input(format!(
            "model `{model}` was not returned by provider `{provider}`; run `asb provider models {provider}` and choose a listed tool-capable model"
        ))),
    }
}

/// Validate replay without contacting a provider: the cassette must contain a
/// request for the selected model that actually includes tool definitions.
pub fn ensure_replay_tool_capable(cassette: &Path, model: &str) -> Result<(), Error> {
    let metadata = std::fs::metadata(cassette).map_err(|error| {
        Error::Input(format!(
            "cannot inspect replay cassette {}: {error}",
            cassette.display()
        ))
    })?;
    if metadata.len() > MAX_CASSETTE_BYTES {
        return Err(Error::Input("replay cassette exceeds 16 MiB".into()));
    }
    let text = std::fs::read_to_string(cassette).map_err(|error| {
        Error::Input(format!(
            "cannot read replay cassette {}: {error}",
            cassette.display()
        ))
    })?;
    let mut saw_model = false;
    for line in text.lines() {
        let value: Value = serde_json::from_str(line)
            .map_err(|_| Error::Input("replay cassette contains invalid JSONL".into()))?;
        let body = value
            .get("request")
            .and_then(|request| request.get("body"))
            .unwrap_or(&value);
        if body.get("model").and_then(Value::as_str) != Some(model) {
            continue;
        }
        saw_model = true;
        let has_tools = body
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty())
            || body.get("tool_choice").is_some()
            || body
                .get("functions")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty());
        if has_tools {
            return Ok(());
        }
    }
    if saw_model {
        Err(Error::Input(format!(
            "replay cassette has model `{model}` but no tool-calling request; record a workload run with a tool-capable model"
        )))
    } else {
        Err(Error::Input(format!(
            "replay cassette has no request for model `{model}`; record the same configuration before replay"
        )))
    }
}

fn fetch(spec: providers::ModelSpec) -> Result<Vec<u8>, Error> {
    let url = spec
        .url()
        .ok_or_else(|| Error::Input("provider model endpoint is not configured".into()))?;
    let config = curl_config(spec, &url, None)?;
    fetch_config(config, spec.name)
}

fn fetch_authenticated(spec: providers::ModelSpec, url: &str, key: &str) -> Result<Vec<u8>, Error> {
    if url.contains(['\n', '\r']) || key.contains(['\n', '\r']) {
        return Err(Error::Input(
            "cli2key endpoint or key contains a newline".into(),
        ));
    }
    let config = curl_config(spec, url, Some(key))?;
    fetch_config(config, spec.name)
}

fn fetch_config(config: String, provider: &str) -> Result<Vec<u8>, Error> {
    let mut command = Command::new("curl");
    command.args([
        "--config",
        "-",
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--connect-timeout",
        "10",
        "--max-time",
        "30",
        "--max-filesize",
        "1048576",
        "--proto",
        "=http,https",
    ]);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::Io(format!(
                    "model discovery needs `curl`, but it could not be started: {error}. Install it, then rerun the command. Suggested command:\n  sudo apt-get update && sudo apt-get install -y curl"
                ))
            } else {
                Error::Io(format!("cannot start curl for model discovery: {error}"))
            }
        })?;
    child
        .stdin
        .take()
        .expect("curl stdin configured")
        .write_all(config.as_bytes())
        .map_err(|error| Error::Io(format!("cannot send model discovery request: {error}")))?;
    let output = child
        .wait_with_output()
        .map_err(|error| Error::Io(format!("cannot finish model discovery request: {error}")))?;
    if !output.status.success() {
        return Err(Error::Input(format!(
            "model discovery request failed for {provider}"
        )));
    }
    if output.stdout.len() > MAX_RESPONSE {
        return Err(Error::Input("provider model response exceeds 1 MiB".into()));
    }
    Ok(output.stdout)
}

fn curl_config(
    spec: providers::ModelSpec,
    url: &str,
    key_override: Option<&str>,
) -> Result<String, Error> {
    if url.contains('\n') || url.contains('\r') {
        return Err(Error::Input(
            "provider endpoint contains invalid newline characters".into(),
        ));
    }
    let mut config = format!("url = \"{}\"\n", escape_config(&url));
    if let Some(credential) = spec.credential {
        let key = match key_override {
            Some(value) => value.to_owned(),
            None => providers::credential(spec.name).ok_or_else(|| {
                Error::Input(format!("missing {credential} or ~/.api_key_{}", spec.name))
            })?,
        };
        if key.contains('\n') || key.contains('\r') {
            return Err(Error::Input(format!(
                "{credential} contains invalid newline characters"
            )));
        }
        let header = match spec.kind {
            providers::ModelKind::Gemini => format!("x-goog-api-key: {key}"),
            providers::ModelKind::Anthropic => format!("x-api-key: {key}"),
            _ => format!("Authorization: Bearer {key}"),
        };
        config.push_str(&format!("header = \"{}\"\n", escape_config(&header)));
    }
    if matches!(spec.kind, providers::ModelKind::Anthropic) {
        config.push_str("header = \"anthropic-version: 2023-06-01\"\n");
    }
    Ok(config)
}

fn local_bridge_executable() -> Option<String> {
    let (root, config) = project::load(Path::new(".")).ok()?;
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(directory) = std::env::var_os("ASB_TOOL_DIR") {
        candidates.push(PathBuf::from(directory).join("codex-bridge"));
    }
    candidates.push(
        project::resolve(&root, &config.tools_dir).join("cli2key/node_modules/.bin/codex-bridge"),
    );
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

fn escape_config(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_models(
    body: &[u8],
    kind: providers::ModelKind,
    provider: &str,
) -> Result<Vec<ModelInfo>, Error> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| Error::Input("provider returned invalid model JSON".into()))?;
    let key = if matches!(kind, providers::ModelKind::Gemini) {
        "models"
    } else {
        "data"
    };
    let entries = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Input("provider response has no model list".into()))?;
    let mut models = entries
        .iter()
        .filter_map(|entry| {
            let id = entry
                .get("id")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)?
                .strip_prefix("models/")
                .unwrap_or_else(|| {
                    entry
                        .get("id")
                        .or_else(|| entry.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                })
                .to_owned();
            Some(ModelInfo {
                tool_calling: supports_tools(entry, kind, provider),
                id,
            })
        })
        .filter(|model| model.tool_calling)
        .collect::<Vec<_>>();
    models.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

fn supports_tools(entry: &Value, kind: providers::ModelKind, provider: &str) -> bool {
    if provider.eq_ignore_ascii_case("cli2key") || matches!(kind, providers::ModelKind::Anthropic) {
        return true;
    }
    for key in ["tool_calling", "tool_calls", "supports_tool_calls"] {
        if entry.get(key).and_then(Value::as_bool) == Some(true) {
            return true;
        }
    }
    if entry
        .get("capabilities")
        .and_then(|capabilities| {
            capabilities
                .get("tools")
                .or_else(|| capabilities.get("tool_calling"))
        })
        .and_then(Value::as_bool)
        == Some(true)
    {
        return true;
    }
    entry
        .get("supported_parameters")
        .and_then(Value::as_array)
        .is_some_and(|parameters| {
            parameters
                .iter()
                .filter_map(Value::as_str)
                .any(|parameter| {
                    matches!(
                        parameter,
                        "tools" | "tool_choice" | "functions" | "function_calling"
                    )
                })
        })
}

fn render_one(
    output: &mut dyn Write,
    provider: &str,
    result: Vec<ModelInfo>,
    json: bool,
) -> Result<(), Error> {
    let names = model_ids(&result);
    if json {
        let value = serde_json::json!({"provider": provider, "models": names});
        serde_json::to_writer_pretty(&mut *output, &value)
            .map_err(|error| Error::Io(error.to_string()))?;
        writeln!(output).map_err(super::io_error)
    } else {
        writeln!(output, "{provider} models ({}):", names.len()).map_err(super::io_error)?;
        for model in names {
            writeln!(output, "{model}").map_err(super::io_error)?;
        }
        Ok(())
    }
}

fn render_all(
    output: &mut dyn Write,
    results: Vec<(String, Result<Vec<ModelInfo>, Error>)>,
    json: bool,
) -> Result<(), Error> {
    if json {
        let value = results
            .into_iter()
            .map(|(name, result)| {
                (
                    name,
                    match result {
                        Ok(models) => {
                            serde_json::json!({"status":"ok", "models":model_ids(&models)})
                        }
                        Err(error) => {
                            serde_json::json!({"status":"unavailable", "reason":error.to_string()})
                        }
                    },
                )
            })
            .collect::<serde_json::Map<_, _>>();
        serde_json::to_writer_pretty(&mut *output, &value)
            .map_err(|error| Error::Io(error.to_string()))?;
        return writeln!(output).map_err(super::io_error);
    }
    for (name, result) in results {
        match result {
            Ok(models) => {
                let models = model_ids(&models);
                writeln!(output, "{name} ({}):", models.len()).map_err(super::io_error)?;
                for model in models {
                    writeln!(output, "  {model}").map_err(super::io_error)?;
                }
            }
            Err(error) => {
                writeln!(output, "{name}: unavailable ({error})").map_err(super::io_error)?;
            }
        }
    }
    Ok(())
}

fn model_ids(models: &[ModelInfo]) -> Vec<String> {
    models.iter().map(|model| model.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_and_gemini_model_shapes() {
        assert_eq!(
            parse_models(
                br#"{"data":[{"id":"z","supported_parameters":["tools"]},{"id":"a","supported_parameters":["tools"]},{"id":"a","supported_parameters":["tools"]}]}"#,
                providers::ModelKind::OpenAi,
                "openai"
            )
            .unwrap()
            .iter()
            .map(|model| model.id.clone())
            .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert_eq!(
            parse_models(
                br#"{"models":[{"name":"models/gemini-2.0","capabilities":{"tools":true}}]}"#,
                providers::ModelKind::Gemini,
                "gemini"
            )
            .unwrap()
            .iter()
            .map(|model| model.id.clone())
            .collect::<Vec<_>>(),
            vec!["gemini-2.0"]
        );
    }

    #[test]
    fn filters_models_without_tool_calling_metadata() {
        let models = parse_models(
            br#"{"data":[{"id":"plain"},{"id":"tools","supported_parameters":["tools"]},{"id":"flag","capabilities":{"tools":true}}]}"#,
            providers::ModelKind::OpenAi,
            "openrouter",
        )
        .expect("model response parses");
        assert_eq!(model_ids(&models), vec!["flag", "tools"]);
    }

    #[test]
    fn replay_requires_a_matching_tool_request() {
        let path = std::env::temp_dir().join(format!("asb-tool-cassette-{}", std::process::id()));
        std::fs::write(
            &path,
            r#"{"request":{"body":{"model":"tool-model","tools":[{"type":"function"}]}}}
"#,
        )
        .expect("cassette writes");
        ensure_replay_tool_capable(&path, "tool-model").expect("tool cassette accepted");
        let error =
            ensure_replay_tool_capable(&path, "other-model").expect_err("wrong model rejected");
        assert!(error.to_string().contains("no request for model"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_provider_suggests_a_close_match() {
        let error = discover("openruter").expect_err("typo rejected");
        assert!(error.to_string().contains("did you mean `openrouter`?"));
    }
}
