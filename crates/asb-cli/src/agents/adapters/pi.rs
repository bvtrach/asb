use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
use std::path::Path;
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "pi",
    "pi",
    Recipe::Npm {
        package: "@mariozechner/pi-coding-agent@0.73.1",
        executable: "pi",
    },
    &[crate::endpoint::RESPONSES, crate::endpoint::CHAT],
    command,
    prepare_adapter,
)
.with_output_usage(output_usage)
.with_execution_failure(execution_failure);

fn execution_failure(stdout: &str, _: &str) -> Option<String> {
    let mut failed = false;
    for event in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        let message = if event["type"] == "turn_end" {
            event.get("message")
        } else if event["type"] == "agent_end" {
            event
                .get("messages")
                .and_then(|messages| messages.as_array())
                .and_then(|messages| {
                    messages
                        .iter()
                        .rev()
                        .find(|message| message["role"] == "assistant")
                })
        } else {
            None
        };
        if let Some(reason) = message
            .and_then(|message| message.get("stopReason"))
            .and_then(|reason| reason.as_str())
        {
            failed = matches!(reason, "error" | "aborted");
        }
    }
    failed.then(|| "agent reported a terminal API request failure or cancellation; inspect the private agent log".into())
}

#[cfg(test)]
mod execution_tests {
    use super::execution_failure;

    #[test]
    fn terminal_api_errors_are_failures_without_echoing_provider_bodies() {
        let output = serde_json::json!({"type":"agent_end","messages":[{"role":"assistant",
            "stopReason":"error","errorMessage":"400 private-provider-body"}]})
        .to_string();
        let failure = execution_failure(&output, "").unwrap();
        assert!(failure.contains("API request"));
        assert!(!failure.contains("private-provider-body"));
    }

    #[test]
    fn recovered_turn_is_not_failed_by_an_earlier_error() {
        let output = [
            serde_json::json!({"type":"turn_end","message":{"stopReason":"error"}}),
            serde_json::json!({"type":"turn_end","message":{"stopReason":"stop"}}),
        ]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        assert!(execution_failure(&output, "").is_none());
    }
}

fn output_usage(stdout: &str, stderr: &str, _: bool) -> crate::measurements::TokenUsage {
    let mut sum = crate::measurements::TokenUsage::default();
    let mut seen = false;
    let mut cache_complete = true;
    for event in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        if event.get("type").and_then(|v| v.as_str()) != Some("turn_end") {
            continue;
        }
        let Some(value) = event
            .get("usage")
            .or_else(|| event.pointer("/message/usage"))
        else {
            return Default::default();
        };
        let number = |key: &str| value.get(key).and_then(|v| v.as_u64());
        let (Some(input), Some(output)) = (number("input"), number("output")) else {
            return Default::default();
        };
        let read = number("cacheRead");
        cache_complete &= read.is_some();
        sum = sum.add(crate::measurements::TokenUsage {
            input_tokens: Some(
                input
                    .saturating_add(read.unwrap_or(0))
                    .saturating_add(number("cacheWrite").unwrap_or(0)),
            ),
            output_tokens: Some(output),
            cached_tokens: read,
            ..Default::default()
        });
        seen = true;
    }
    if seen {
        if !cache_complete {
            sum.cached_tokens = None;
        }
        sum.with_derived_total()
    } else {
        crate::collectors::adapters::token_usage::usage_from_text(&format!("{stdout}\n{stderr}"))
    }
}
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "pi".into(),
        args: vec![
            "--provider".into(),
            "asb".into(),
            "--model".into(),
            context.config.model.clone(),
            "--mode".into(),
            "json".into(),
            "--print".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let directory = context.runtime_dir.join("pi/config");
    let api = match connection.interface {
        crate::endpoint::Interface::OpenAiResponses => "openai-responses",
        crate::endpoint::Interface::OpenAiChat => "openai-completions",
        _ => unreachable!("Pi advertises only OpenAI-compatible interfaces"),
    };
    context.command.env(
        "ASB_AGENT_API_KEY",
        connection.credential.as_deref().unwrap_or("asb-local"),
    );
    configure_provider(
        context.command,
        &directory,
        "asb",
        context.model,
        &connection.base_url,
        api,
        "ASB_AGENT_API_KEY",
    )
}

fn configure_provider(
    command: &mut std::process::Command,
    directory: &Path,
    provider: &str,
    model: &str,
    base_url: &str,
    api: &str,
    api_key: &str,
) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Pi provider configuration directory: {error}"
        ))
    })?;
    let configuration = serde_json::json!({
        "providers": { provider: {
            "baseUrl": base_url, "api": api, "apiKey": api_key,
            "models": [{
                "id": model, "name": model, "reasoning": true, "input": ["text"],
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
                "contextWindow": 128000, "maxTokens": 16384
            }]
        }}
    });
    let path = directory.join("models.json");
    let content = serde_json::to_vec_pretty(&configuration)
        .map_err(|error| Error::Run(format!("cannot render Pi provider configuration: {error}")))?;
    std::fs::write(&path, content)
        .map_err(|error| Error::Run(format!("cannot write {}: {error}", path.display())))?;
    command.env("PI_CODING_AGENT_DIR", directory);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_pi_adapter() {
        let directory = std::env::temp_dir().join(format!("asb-pi-config-{}", std::process::id()));
        let mut command = std::process::Command::new("pi");
        configure_provider(
            &mut command,
            &directory,
            "cli2key",
            "test-model",
            "http://127.0.0.1:3456/v1",
            "openai-responses",
            "CLI2KEY_API_KEY",
        )
        .expect("configuration succeeds");
        let text =
            std::fs::read_to_string(directory.join("models.json")).expect("configuration exists");
        assert!(text.contains("test-model"));
        let _ = std::fs::remove_dir_all(directory);
    }
}
