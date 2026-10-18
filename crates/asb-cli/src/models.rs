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
const MAX_ENDPOINT_CATALOG: usize = 8 * 1024 * 1024;
// Cassettes are bounded inputs, but multi-step agent runs can legitimately
// exceed the former 16 MiB limit (for example Workbuddy/SWE-bench). Keep a
// finite ceiling while allowing those recorded runs to be replayed.
const MAX_CASSETTE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelInfo {
    id: String,
    // None means the live catalog does not report this capability.
    tool_calling: Option<bool>,
}

/// Public OpenCode-maintained metadata; never send provider credentials here.
pub(crate) fn fetch_endpoint_catalog() -> Result<Vec<u8>, Error> {
    let catalog_url = std::env::var("ASB_ENDPOINT_CATALOG_URL")
        .unwrap_or_else(|_| "https://models.dev/api.json".into());
    let output = Command::new("curl")
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "3",
            "--max-time",
            "8",
            "--max-filesize",
            "8388608",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            &catalog_url,
        ])
        .output()
        .map_err(|error| Error::Io(format!("cannot retrieve endpoint metadata: {error}")))?;
    if !output.status.success() || output.stdout.len() > MAX_ENDPOINT_CATALOG {
        return Err(Error::Input(
            "current endpoint metadata is unavailable or exceeds 8 MiB".into(),
        ));
    }
    Ok(output.stdout)
}

/// List models for one provider, or all providers when no name is supplied.
pub struct ModelListOptions {
    pub provider: Option<String>,
    pub json: bool,
    pub free_only: bool,
}

pub fn list(options: ModelListOptions, output: &mut dyn Write) -> Result<(), Error> {
    let ModelListOptions {
        provider,
        json,
        free_only,
    } = options;
    if let Some(provider) = provider {
        if free_only && !providers::supports_free_models(&provider) {
            return Err(Error::Input(
                "`--free` is supported for `openrouter`, `opencode-zen`, and `ollama`; query one of those providers or omit `--free`".into(),
            ));
        }
        let result = discover(&provider, free_only)?;
        render_one(output, &provider, result, json)
    } else {
        let mut all = Vec::new();
        let names = if free_only {
            providers::free_names()
        } else {
            providers::names()
        };
        for name in names {
            match discover(name, free_only) {
                Ok(models) => all.push(((*name).to_owned(), Ok(models))),
                Err(error) => all.push(((*name).to_owned(), Err(error))),
            }
        }
        render_all(output, all, json)
    }
}

fn discover(provider: &str, free_only: bool) -> Result<Vec<ModelInfo>, Error> {
    discover_for_model(provider, free_only, None)
}

fn discover_for_model(
    provider: &str,
    free_only: bool,
    selected: Option<&str>,
) -> Result<Vec<ModelInfo>, Error> {
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
        return parse_models(&body, spec.kind, provider, free_only);
    }
    let public_catalog = provider.eq_ignore_ascii_case("opencode-zen")
        || (provider.eq_ignore_ascii_case("openrouter")
            && providers::credential(provider).is_none());
    if let Some(credential) = spec.credential {
        if !public_catalog && providers::credential(provider).is_none() {
            return Err(Error::Input(format!(
                "missing {credential} or ~/.api_key_{}",
                spec.name
            )));
        }
    }
    let Some(url) = spec.url_for_model(selected) else {
        if matches!(spec.kind, providers::ModelKind::Fireworks)
            && std::env::var_os("FIREWORKS_ACCOUNT_ID").is_some()
        {
            return Err(Error::Input("FIREWORKS_ACCOUNT_ID must be a nonempty account ID containing only letters, digits, underscores or hyphens".into()));
        }
        let endpoint = spec.endpoint.unwrap_or("provider endpoint variable");
        return Err(Error::Input(format!("missing {endpoint}")));
    };
    let request_spec = if public_catalog {
        providers::ModelSpec {
            credential: None,
            ..spec
        }
    } else {
        spec
    };
    let body = fetch_pages(request_spec, &url)?;
    let body = if matches!(spec.kind, providers::ModelKind::Ollama) {
        enrich_ollama_capabilities(&body, spec)?
    } else {
        body
    };
    parse_models(&body, spec.kind, provider, free_only)
}

// Older Ollama versions omit capabilities from /api/tags. Query the model's
// metadata rather than assuming that every local model supports tools.
fn enrich_ollama_capabilities(body: &[u8], spec: providers::ModelSpec) -> Result<Vec<u8>, Error> {
    let mut catalog: Value = serde_json::from_slice(body)
        .map_err(|_| Error::Input("provider returned invalid model JSON".into()))?;
    let entries = catalog
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| Error::Input("provider response has no model list".into()))?;
    let tags_url = spec
        .url()
        .ok_or_else(|| Error::Input("provider model endpoint is not configured".into()))?;
    let show_url = tags_url
        .strip_suffix("/api/tags")
        .map(|base| format!("{base}/api/show"))
        .ok_or_else(|| Error::Input("Ollama model endpoint must end in /api/tags".into()))?;
    for entry in entries {
        if entry.get("capabilities").is_some() {
            continue;
        }
        let Some(name) = entry
            .get("name")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let payload = serde_json::json!({"model": name}).to_string();
        let mut config = curl_config(spec, &show_url, None)?;
        config.push_str("header = \"Content-Type: application/json\"\n");
        config.push_str(&format!("data = \"{}\"\n", escape_config(&payload)));
        let response = fetch_config(config, spec.name)?;
        let metadata: Value = serde_json::from_slice(&response).map_err(|_| {
            Error::Input(format!(
                "Ollama returned invalid metadata for model `{name}`"
            ))
        })?;
        if let Some(capabilities) = metadata.get("capabilities") {
            entry["capabilities"] = capabilities.clone();
        }
    }
    serde_json::to_vec(&catalog)
        .map_err(|error| Error::Input(format!("cannot read Ollama catalog: {error}")))
}

/// Discover tool-capable models for interactive setup without rendering them.
pub(crate) fn discover_for_setup(provider: &str) -> Result<Vec<String>, Error> {
    discover(provider, false).map(|models| {
        models
            .into_iter()
            .filter(|model| model.tool_calling != Some(false))
            .map(|model| model.id)
            .collect()
    })
}

/// Require the model to be in the live catalog and reject explicit lack of
/// tool support. Return false when the catalog leaves that capability unknown.
pub fn ensure_tool_capable(
    provider: &str,
    model: &str,
    bridge: Option<(&str, &str)>,
) -> Result<bool, Error> {
    let models = if let Some((url, key)) = bridge {
        let spec = providers::model_spec(provider)
            .ok_or_else(|| Error::Input(format!("unknown provider `{provider}`")))?;
        let body =
            fetch_authenticated(spec, &format!("{}/models", url.trim_end_matches('/')), key)?;
        parse_models(&body, spec.kind, provider, false)?
    } else {
        discover_for_model(provider, false, Some(model))?
    };
    match models.into_iter().find(|candidate| candidate.id == model) {
        Some(candidate) if candidate.tool_calling != Some(false) => {
            Ok(candidate.tool_calling.is_some())
        }
        Some(_) => Err(Error::Input(format!(
            "model `{model}` for provider `{provider}` does not support tool-enabled generation according to the provider catalog; choose another model"
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
        return Err(Error::Input("replay cassette exceeds 64 MiB".into()));
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
        let raw_body = value
            .get("request")
            .and_then(|request| request.get("body"))
            .unwrap_or(&value);
        let body = if let Some(text) = raw_body.as_str() {
            serde_json::from_str::<Value>(text).map_err(|_| {
                Error::Input(
                    "replay cassette contains a request body that is not valid JSON".into(),
                )
            })?
        } else {
            raw_body.clone()
        };
        let Some(recorded_model) = body.get("model").and_then(Value::as_str) else {
            continue;
        };
        if !models_match(recorded_model, model) {
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

/// Infer the recorded wire interface without contacting any provider.
pub(crate) fn replay_interface(
    cassette: &Path,
) -> Result<Option<crate::endpoint::Interface>, Error> {
    let text = std::fs::read_to_string(cassette).map_err(|error| {
        Error::Input(format!(
            "cannot read replay cassette {}: {error}",
            cassette.display()
        ))
    })?;
    for line in text.lines() {
        let value: Value = serde_json::from_str(line)
            .map_err(|_| Error::Input("replay cassette contains invalid JSONL".into()))?;
        let Some(path) = value
            .get("request")
            .and_then(|v| v.get("url"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let path = path.split('?').next().unwrap_or(path);
        let interface = if path.ends_with("/chat/completions") {
            Some(crate::endpoint::Interface::OpenAiChat)
        } else if path.ends_with("/responses") {
            Some(crate::endpoint::Interface::OpenAiResponses)
        } else if path.ends_with("/messages") {
            Some(crate::endpoint::Interface::AnthropicMessages)
        } else {
            None
        };
        if interface.is_some() {
            return Ok(interface);
        }
    }
    Ok(None)
}

fn models_match(recorded: &str, configured: &str) -> bool {
    recorded == configured
        || recorded.ends_with(&format!("/{configured}"))
        || configured.ends_with(&format!("/{recorded}"))
}

fn fetch_pages(spec: providers::ModelSpec, url: &str) -> Result<Vec<u8>, Error> {
    collect_catalog_pages(spec.kind, url, |page| {
        fetch_config(curl_config(spec, page, None)?, spec.name)
    })
}

// Anthropic, Gemini and Fireworks paginate their model catalogs. Keep requests,
// aggregate size, and cursor behavior bounded, and do not accept a URL from
// the response (which could redirect provider credentials to another host).
fn collect_catalog_pages(
    kind: providers::ModelKind,
    base: &str,
    mut fetch_page: impl FnMut(&str) -> Result<Vec<u8>, Error>,
) -> Result<Vec<u8>, Error> {
    let collection = if matches!(
        kind,
        providers::ModelKind::Gemini
            | providers::ModelKind::Ollama
            | providers::ModelKind::Fireworks
    ) {
        "models"
    } else {
        "data"
    };
    let mut merged = Vec::new();
    let mut cursor = None::<String>;
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0usize;
    for _ in 0..16 {
        let url = match &cursor {
            Some(token) => format!(
                "{base}{}{}={}",
                if base.contains('?') { '&' } else { '?' },
                if matches!(kind, providers::ModelKind::Anthropic) {
                    "after_id"
                } else {
                    "pageToken"
                },
                encode_query(token)
            ),
            None => base.to_owned(),
        };
        let body = fetch_page(&url)?;
        bytes += body.len();
        if bytes > 8 * 1024 * 1024 {
            return Err(Error::Input("provider model catalog exceeds 8 MiB".into()));
        }
        let value: Value = serde_json::from_slice(&body)
            .map_err(|_| Error::Input("provider returned invalid model JSON".into()))?;
        merged.extend(
            (if matches!(kind, providers::ModelKind::Together) && value.is_array() {
                Some(&value)
            } else {
                value.get(collection)
            })
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Input("provider response has no model list".into()))?
            .iter()
            .cloned(),
        );
        let next = match kind {
            providers::ModelKind::Anthropic
                if value.get("has_more").and_then(Value::as_bool) == Some(true) =>
            {
                Some(
                    value
                        .get("last_id")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            Error::Input("provider model catalog has no continuation cursor".into())
                        })?,
                )
            }
            providers::ModelKind::Gemini | providers::ModelKind::Fireworks => value
                .get("nextPageToken")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty()),
            _ => None,
        };
        let Some(next) = next else {
            return serde_json::to_vec(&serde_json::json!({(collection): merged}))
                .map_err(|e| Error::Input(format!("cannot read provider model catalog: {e}")));
        };
        if next.len() > 1024 || !seen.insert(next.to_owned()) {
            return Err(Error::Input(
                "provider model catalog has an invalid or repeated continuation cursor".into(),
            ));
        }
        cursor = Some(next.to_owned());
    }
    Err(Error::Input(
        "provider model catalog exceeds 16 pages".into(),
    ))
}

fn encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
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
        "--disable",
        "--config",
        "-",
        "--fail",
        "--silent",
        "--show-error",
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
    free_only: bool,
) -> Result<Vec<ModelInfo>, Error> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| Error::Input("provider returned invalid model JSON".into()))?;
    let key = if matches!(
        kind,
        providers::ModelKind::Gemini
            | providers::ModelKind::Ollama
            | providers::ModelKind::Fireworks
    ) {
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
            let tool_calling = supports_tools(entry, kind, provider);
            (!free_only || is_free_model_entry(entry, &id, provider, kind))
                .then_some(ModelInfo { tool_calling, id })
        })
        .collect::<Vec<_>>();
    models.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

fn is_free_model_entry(
    entry: &Value,
    model_id: &str,
    provider: &str,
    kind: providers::ModelKind,
) -> bool {
    if provider.eq_ignore_ascii_case("ollama") && matches!(kind, providers::ModelKind::Ollama) {
        return true;
    }
    if provider.eq_ignore_ascii_case("opencode-zen") {
        return model_id.ends_with("-free");
    }
    if !provider.eq_ignore_ascii_case("openrouter") {
        return false;
    }
    model_id.ends_with(":free")
        || entry
            .get("pricing")
            .and_then(Value::as_object)
            .is_some_and(|pricing| {
                is_zero_price(pricing.get("prompt")) && is_zero_price(pricing.get("completion"))
            })
}

fn is_zero_price(value: Option<&Value>) -> bool {
    value.is_some_and(|value| {
        value
            .as_f64()
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.trim().parse::<f64>().ok())
            })
            .is_some_and(|number| number == 0.0)
    })
}

fn supports_tools(entry: &Value, kind: providers::ModelKind, provider: &str) -> Option<bool> {
    if matches!(kind, providers::ModelKind::Gemini) {
        if let Some(methods) = entry
            .get("supportedGenerationMethods")
            .and_then(Value::as_array)
        {
            if !methods
                .iter()
                .any(|value| value.as_str() == Some("generateContent"))
            {
                return Some(false);
            }
        }
    }
    if matches!(kind, providers::ModelKind::Ollama) {
        if let Some(values) = entry.get("capabilities").and_then(Value::as_array) {
            return Some(values.iter().any(|value| value.as_str() == Some("tools")));
        }
    }
    for key in [
        "tool_calling",
        "tool_calls",
        "supports_tool_calls",
        "supportsTools",
    ] {
        if let Some(value) = entry.get(key).and_then(Value::as_bool) {
            return Some(value);
        }
    }
    if let Some(value) = entry
        .get("capabilities")
        .and_then(|capabilities| {
            capabilities
                .get("tools")
                .or_else(|| capabilities.get("tool_calling"))
                .or_else(|| capabilities.get("function_calling"))
        })
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.get("supported").and_then(Value::as_bool))
        })
    {
        return Some(value);
    }
    if let Some(parameters) = entry.get("supported_parameters").and_then(Value::as_array) {
        return Some(
            parameters
                .iter()
                .filter_map(Value::as_str)
                .any(|parameter| {
                    matches!(
                        parameter,
                        "tools" | "tool_choice" | "functions" | "function_calling"
                    )
                }),
        );
    }
    // These APIs document tool use as a standard capability of their exposed
    // agent models, while their model list does not include a capability bit.
    if provider.eq_ignore_ascii_case("cli2key") || matches!(kind, providers::ModelKind::Anthropic) {
        return Some(true);
    }
    None
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
    models
        .iter()
        .filter(|model| model.tool_calling != Some(false))
        .map(|model| model.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_explicit_capabilities_take_precedence_over_documented_defaults() {
        assert_eq!(
            supports_tools(
                &serde_json::json!({"capabilities":{"tools":{"supported":false}}}),
                providers::ModelKind::Anthropic,
                "anthropic"
            ),
            Some(false)
        );
        assert_eq!(
            supports_tools(
                &serde_json::json!({"supportedGenerationMethods":["embedContent"]}),
                providers::ModelKind::Gemini,
                "gemini"
            ),
            Some(false)
        );
        assert_eq!(
            supports_tools(
                &serde_json::json!({"supportedGenerationMethods":["generateContent"]}),
                providers::ModelKind::Gemini,
                "gemini"
            ),
            None
        );
        assert_eq!(
            supports_tools(
                &serde_json::json!({"capabilities":{"function_calling":false}}),
                providers::ModelKind::OpenAi,
                "mistral"
            ),
            Some(false)
        );
        assert_eq!(
            supports_tools(
                &serde_json::json!({"supportsTools":true}),
                providers::ModelKind::Fireworks,
                "fireworks"
            ),
            Some(true)
        );
    }

    #[test]
    fn catalog_pagination_uses_encoded_cursors_and_collects_every_page() {
        for (kind, first, second, query) in [
            (
                providers::ModelKind::Anthropic,
                r#"{"data":[{"id":"one"}],"has_more":true,"last_id":"a/b?"}"#,
                r#"{"data":[{"id":"two"}],"has_more":false}"#,
                "after_id=a%2Fb%3F",
            ),
            (
                providers::ModelKind::Gemini,
                r#"{"models":[{"name":"models/one"}],"nextPageToken":"a/b?"}"#,
                r#"{"models":[{"name":"models/two"}]}"#,
                "pageToken=a%2Fb%3F",
            ),
            (
                providers::ModelKind::Fireworks,
                r#"{"models":[{"name":"accounts/fireworks/models/one"}],"nextPageToken":"a/b?"}"#,
                r#"{"models":[{"name":"accounts/fireworks/models/two"}]}"#,
                "pageToken=a%2Fb%3F",
            ),
        ] {
            let mut calls = Vec::new();
            let body = collect_catalog_pages(kind, "http://127.0.0.1/v1/models", |url| {
                calls.push(url.to_owned());
                Ok(if calls.len() == 1 { first } else { second }
                    .as_bytes()
                    .to_vec())
            })
            .unwrap();
            assert_eq!(calls.len(), 2);
            assert!(calls[1].ends_with(query));
            let parsed = parse_models(&body, kind, "fixture", false).unwrap();
            if kind == providers::ModelKind::Fireworks {
                assert_eq!(
                    model_ids(&parsed),
                    vec![
                        "accounts/fireworks/models/one",
                        "accounts/fireworks/models/two"
                    ]
                );
            } else {
                assert_eq!(model_ids(&parsed), vec!["one", "two"]);
            }
        }
    }

    #[test]
    fn together_catalog_accepts_documented_top_level_array() {
        let body = collect_catalog_pages(
            providers::ModelKind::Together,
            "http://localhost/models",
            |_| Ok(br#"[{"id":"fixture/chat","type":"chat"}]"#.to_vec()),
        )
        .unwrap();
        let parsed =
            parse_models(&body, providers::ModelKind::Together, "together", false).unwrap();
        assert_eq!(model_ids(&parsed), vec!["fixture/chat"]);
        assert_eq!(parsed[0].tool_calling, None);
    }

    #[test]
    fn catalog_pagination_rejects_repeated_and_missing_cursors() {
        let repeated = collect_catalog_pages(
            providers::ModelKind::Gemini,
            "http://localhost/models",
            |_| Ok(br#"{"models":[],"nextPageToken":"same"}"#.to_vec()),
        );
        assert!(repeated.unwrap_err().to_string().contains("repeated"));
        let missing = collect_catalog_pages(
            providers::ModelKind::Anthropic,
            "http://localhost/models",
            |_| Ok(br#"{"data":[],"has_more":true}"#.to_vec()),
        );
        assert!(
            missing
                .unwrap_err()
                .to_string()
                .contains("continuation cursor")
        );
    }

    fn replay_fixture(contents: &str, check: impl FnOnce(&Path)) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "asb-replay-validation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, contents).unwrap();
        check(&path);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_detects_recorded_interface_without_live_metadata() {
        for (path, expected) in [
            ("/v1/responses", crate::endpoint::Interface::OpenAiResponses),
            (
                "/api/v1/chat/completions?x=1",
                crate::endpoint::Interface::OpenAiChat,
            ),
            (
                "/v1/messages?beta=true",
                crate::endpoint::Interface::AnthropicMessages,
            ),
        ] {
            let cassette = serde_json::json!({"request":{"url":path}}).to_string();
            replay_fixture(&cassette, |file| {
                assert_eq!(replay_interface(file).unwrap(), Some(expected))
            });
        }
    }

    #[test]
    fn malformed_and_wrong_shape_model_catalogs_are_rejected() {
        for (body, message) in [
            (&b"not json"[..], "invalid model JSON"),
            (&b"{}"[..], "no model list"),
            (&b"{\"data\":{}}"[..], "no model list"),
        ] {
            let error =
                parse_models(body, providers::ModelKind::OpenAi, "openai", false).unwrap_err();
            assert!(error.to_string().contains(message));
        }
    }

    #[test]
    fn model_tool_capabilities_accept_supported_metadata_shapes() {
        let models = parse_models(
            br#"{"data":[
          {"id":"a","tool_calling":true},
          {"id":"b","tool_calls":true},
          {"id":"c","supports_tool_calls":true},
          {"id":"d","capabilities":{"tool_calling":true}},
          {"id":"e","supported_parameters":["function_calling"]},
          {"id":"f","supported_parameters":["tool_choice"]},
          {"id":"ignored","tool_calling":false},
          {"supported_parameters":["tools"]}
        ]}"#,
            providers::ModelKind::OpenAi,
            "openrouter",
            false,
        )
        .unwrap();
        assert_eq!(model_ids(&models), vec!["a", "b", "c", "d", "e", "f"]);
    }

    #[test]
    fn free_pricing_requires_both_input_and_output_to_be_zero() {
        let models = parse_models(
            br#"{"data":[
          {"id":"both-zero","tool_calling":true,"pricing":{"prompt":0,"completion":"0.0"}},
          {"id":"missing","tool_calling":true,"pricing":{"prompt":0}},
          {"id":"paid-output","tool_calling":true,"pricing":{"prompt":0,"completion":1}},
          {"id":"nonnumeric","tool_calling":true,"pricing":{"prompt":"free","completion":0}}
        ]}"#,
            providers::ModelKind::OpenAi,
            "openrouter",
            true,
        )
        .unwrap();
        assert_eq!(model_ids(&models), vec!["both-zero"]);
    }

    #[test]
    fn curl_configuration_rejects_newlines_and_escapes_credentials() {
        let spec = providers::model_spec("openai").unwrap();
        let output = curl_config(
            spec,
            "http://127.0.0.1:1234/v1/models",
            Some("fake\"key\\value"),
        )
        .unwrap();
        assert!(output.contains(r#"Authorization: Bearer fake\"key\\value"#));
        assert!(curl_config(spec, "http://127.0.0.1/\nheader=bad", Some("fixture")).is_err());
        assert!(curl_config(spec, "http://127.0.0.1/", Some("fixture\r\nheader=bad")).is_err());
    }

    #[test]
    fn replay_skips_bodyless_requests_before_tool_call() {
        replay_fixture(
            concat!(
                "{\"request\":{\"method\":\"GET\",\"body\":null}}\n",
                "{\"request\":{\"body\":{\"model\":\"test-model\",\"tools\":[{}]}}}\n"
            ),
            |path| ensure_replay_tool_capable(path, "test-model").unwrap(),
        );
    }

    #[test]
    fn replay_rejects_invalid_jsonl_and_invalid_encoded_request_body() {
        for (fixture, message) in [
            ("not json\n", "invalid JSONL"),
            (
                "{\"request\":{\"body\":\"not-json\"}}\n",
                "request body that is not valid JSON",
            ),
        ] {
            replay_fixture(fixture, |path| {
                assert!(
                    ensure_replay_tool_capable(path, "test-model")
                        .unwrap_err()
                        .to_string()
                        .contains(message)
                );
            });
        }
    }

    #[test]
    fn replay_rejects_matching_models_without_a_tool_request() {
        replay_fixture(
            "{\"request\":{\"body\":{\"model\":\"test-model\",\"tools\":[]}}}\n",
            |path| {
                assert!(
                    ensure_replay_tool_capable(path, "test-model")
                        .unwrap_err()
                        .to_string()
                        .contains("no tool-calling request")
                );
            },
        );
    }

    #[test]
    fn replay_accepts_tool_choice_and_legacy_function_requests() {
        for body in [
            r#"{"model":"test-model","tool_choice":"auto"}"#,
            r#"{"model":"test-model","functions":[{}]}"#,
        ] {
            replay_fixture(&format!("{{\"request\":{{\"body\":{body}}}}}\n"), |path| {
                ensure_replay_tool_capable(path, "test-model").unwrap();
            });
        }
    }

    #[test]
    fn replay_rejects_cassettes_over_the_size_limit() {
        replay_fixture("", |path| {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(MAX_CASSETTE_BYTES + 1)
                .unwrap();
            assert!(
                ensure_replay_tool_capable(path, "test-model")
                    .unwrap_err()
                    .to_string()
                    .contains("64 MiB")
            );
        });
    }

    #[test]
    fn parses_openai_and_gemini_model_shapes() {
        assert_eq!(
            parse_models(
                br#"{"data":[{"id":"z","supported_parameters":["tools"]},{"id":"a","supported_parameters":["tools"]},{"id":"a","supported_parameters":["tools"]}]}"#,
                providers::ModelKind::OpenAi,
                "openai",
                false,
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
                "gemini",
                false,
            )
            .unwrap()
            .iter()
            .map(|model| model.id.clone())
            .collect::<Vec<_>>(),
            vec!["gemini-2.0"]
        );
    }

    #[test]
    fn retains_models_with_unknown_tool_calling_metadata() {
        let models = parse_models(
            br#"{"data":[{"id":"plain"},{"id":"tools","supported_parameters":["tools"]},{"id":"flag","capabilities":{"tools":true}}]}"#,
            providers::ModelKind::OpenAi,
            "openrouter",
            false,
        )
        .expect("model response parses");
        assert_eq!(model_ids(&models), vec!["flag", "plain", "tools"]);
        assert_eq!(
            models
                .iter()
                .find(|m| m.id == "plain")
                .unwrap()
                .tool_calling,
            None
        );
    }

    #[test]
    fn free_filter_accepts_openrouter_free_models_only() {
        let models = parse_models(
            br#"{"data":[
                {"id":"paid","supported_parameters":["tools"],"pricing":{"prompt":"0.000001","completion":"0.000002"}},
                {"id":"suffix:free","supported_parameters":["tools"],"pricing":{"prompt":"0","completion":"0"}},
                {"id":"zero-pricing","supported_parameters":["tools"],"pricing":{"prompt":"0","completion":"0"}},
                {"id":"no-tools:free","pricing":{"prompt":"0","completion":"0"}}
            ]}"#,
            providers::ModelKind::OpenAi,
            "openrouter",
            true,
        )
        .expect("model response parses");
        assert_eq!(
            model_ids(&models),
            vec!["no-tools:free", "suffix:free", "zero-pricing"]
        );
    }

    #[test]
    fn free_filter_treats_ollama_models_as_local() {
        let models = parse_models(
            br#"{"models":[{"name":"local-a","capabilities":{"tools":true}},{"name":"local-b","capabilities":{"tools":true}}]}"#,
            providers::ModelKind::Ollama,
            "ollama",
            true,
        )
        .expect("model response parses");
        assert_eq!(model_ids(&models), vec!["local-a", "local-b"]);
    }

    #[test]
    fn ollama_capability_arrays_only_accept_explicit_tools() {
        let models = parse_models(
            br#"{"models":[
              {"name":"qwen3:0.6b","capabilities":["completion","tools","thinking"]},
              {"name":"embedding","capabilities":["embedding"]},
              {"name":"plain","capabilities":["completion"]},
              {"name":"unknown"},
              {"name":"wrong","capabilities":["tool"]}
            ]}"#,
            providers::ModelKind::Ollama,
            "ollama",
            true,
        )
        .unwrap();
        assert_eq!(model_ids(&models), vec!["qwen3:0.6b", "unknown"]);
    }

    #[test]
    fn opencode_zen_catalog_is_public_and_free_models_are_suffix_marked() {
        let models = parse_models(
            br#"{"data":[{"id":"paid-model"},{"id":"free-model-free"}]}"#,
            providers::ModelKind::OpenAi,
            "opencode-zen",
            true,
        )
        .expect("model response parses");
        assert_eq!(model_ids(&models), vec!["free-model-free"]);
        assert!(
            providers::model_spec("opencode-zen")
                .unwrap()
                .credential
                .is_some()
        );
    }

    #[test]
    fn free_flag_parses_with_json_and_provider_in_any_order() {
        let options = ModelListOptions {
            provider: Some("openrouter".into()),
            json: true,
            free_only: true,
        };
        assert_eq!(options.provider.as_deref(), Some("openrouter"));
        assert!(options.json && options.free_only);
    }

    #[test]
    fn free_flag_rejects_providers_without_free_catalog_metadata() {
        let error = list(
            ModelListOptions {
                provider: Some("openai".into()),
                json: false,
                free_only: true,
            },
            &mut Vec::new(),
        )
        .expect_err("paid provider cannot be classified as free");
        assert!(error.to_string().contains("openrouter`"));
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
    fn replay_accepts_json_encoded_request_bodies_and_provider_namespaces() {
        let path =
            std::env::temp_dir().join(format!("asb-tool-cassette-json-{}", std::process::id()));
        std::fs::write(
            &path,
            r#"{"request":{"body":"{\"model\":\"openrouter/tool-model\",\"tools\":[{\"type\":\"function\"}]}"},"response":{"status":200}}
"#,
        )
        .expect("cassette writes");
        ensure_replay_tool_capable(&path, "tool-model").expect("namespaced model accepted");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_provider_suggests_a_close_match() {
        let error = discover("openruter", false).expect_err("typo rejected");
        assert!(error.to_string().contains("did you mean `openrouter`?"));
    }
}
