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

fn discover(provider: &str) -> Result<Vec<String>, Error> {
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
        return parse_models(&body, spec.kind);
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
    parse_models(&body, spec.kind)
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

fn parse_models(body: &[u8], kind: providers::ModelKind) -> Result<Vec<String>, Error> {
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
        .filter_map(|entry| entry.get("id").or_else(|| entry.get("name")))
        .filter_map(Value::as_str)
        .map(|name| name.strip_prefix("models/").unwrap_or(name).to_owned())
        .collect::<Vec<_>>();
    models.sort_unstable();
    models.dedup();
    Ok(models)
}

fn render_one(
    output: &mut dyn Write,
    provider: &str,
    result: Vec<String>,
    json: bool,
) -> Result<(), Error> {
    if json {
        let value = serde_json::json!({"provider": provider, "models": result});
        serde_json::to_writer_pretty(&mut *output, &value)
            .map_err(|error| Error::Io(error.to_string()))?;
        writeln!(output).map_err(super::io_error)
    } else {
        writeln!(output, "{provider} models ({}):", result.len()).map_err(super::io_error)?;
        for model in result {
            writeln!(output, "{model}").map_err(super::io_error)?;
        }
        Ok(())
    }
}

fn render_all(
    output: &mut dyn Write,
    results: Vec<(String, Result<Vec<String>, Error>)>,
    json: bool,
) -> Result<(), Error> {
    if json {
        let value = results
            .into_iter()
            .map(|(name, result)| {
                (
                    name,
                    match result {
                        Ok(models) => serde_json::json!({"status":"ok", "models":models}),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_and_gemini_model_shapes() {
        assert_eq!(
            parse_models(
                br#"{"data":[{"id":"z"},{"id":"a"},{"id":"a"}]}"#,
                providers::ModelKind::OpenAi
            )
            .unwrap(),
            vec!["a", "z"]
        );
        assert_eq!(
            parse_models(
                br#"{"models":[{"name":"models/gemini-2.0"}]}"#,
                providers::ModelKind::Gemini
            )
            .unwrap(),
            vec!["gemini-2.0"]
        );
    }

    #[test]
    fn unsupported_provider_suggests_a_close_match() {
        let error = discover("openruter").expect_err("typo rejected");
        assert!(error.to_string().contains("did you mean `openrouter`?"));
    }
}
