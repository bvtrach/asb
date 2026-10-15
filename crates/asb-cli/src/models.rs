//! Live model discovery through provider model-list endpoints.

use crate::Error;
use crate::providers;
use serde_json::Value;
use std::io::Write;
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
            return Err(Error::Usage("models accepts PROVIDER and --json"));
        }
    }
    Ok((provider, json))
}

fn discover(provider: &str) -> Result<Vec<String>, Error> {
    let spec = providers::model_spec(provider)
        .ok_or_else(|| Error::Input(format!("unsupported provider: {provider}")))?;
    if let Some(credential) = spec.credential {
        if std::env::var_os(credential).is_none() {
            return Err(Error::Input(format!("missing {credential}")));
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
    let config = curl_config(spec)?;
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
        .map_err(|error| Error::Io(format!("cannot start curl for model discovery: {error}")))?;
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
            "model discovery request failed for {}",
            spec.name
        )));
    }
    if output.stdout.len() > MAX_RESPONSE {
        return Err(Error::Input("provider model response exceeds 1 MiB".into()));
    }
    Ok(output.stdout)
}

fn curl_config(spec: providers::ModelSpec) -> Result<String, Error> {
    let url = spec
        .url()
        .ok_or_else(|| Error::Input("provider model endpoint is not configured".into()))?;
    if url.contains('\n') || url.contains('\r') {
        return Err(Error::Input(
            "provider endpoint contains invalid newline characters".into(),
        ));
    }
    let mut config = format!("url = \"{}\"\n", escape_config(&url));
    if let Some(credential) = spec.credential {
        let key =
            std::env::var(credential).map_err(|_| Error::Input(format!("missing {credential}")))?;
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
}
