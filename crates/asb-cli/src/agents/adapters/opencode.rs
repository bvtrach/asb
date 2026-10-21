use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "opencode",
    "opencode",
    Recipe::Npm {
        package: "opencode-ai@1.19.0",
        executable: "opencode",
    },
    &[
        crate::endpoint::CHAT,
        crate::endpoint::MESSAGES_KEY,
        crate::endpoint::RESPONSES,
    ],
    command,
    prepare_adapter,
)
.with_output_usage(output_usage)
.with_state_environment_vars(&[
    "OPENCODE_CONFIG_CONTENT",
    "OPENCODE_CONFIG",
    "OPENCODE_CONFIG_DIR",
]);

fn output_usage(stdout: &str, stderr: &str, _: bool) -> crate::measurements::TokenUsage {
    let mut sum = crate::measurements::TokenUsage::default();
    let mut seen = false;
    let mut cache_complete = true;
    for event in stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        if event.get("type").and_then(|v| v.as_str()) != Some("step_finish") {
            continue;
        }
        let Some(tokens) = event.pointer("/part/tokens") else {
            return Default::default();
        };
        let number = |name: &str| tokens.get(name).and_then(|v| v.as_u64());
        let (Some(input), Some(output)) = (number("input"), number("output")) else {
            return Default::default();
        };
        let read = tokens.pointer("/cache/read").and_then(|v| v.as_u64());
        let write = tokens.pointer("/cache/write").and_then(|v| v.as_u64());
        cache_complete &= read.is_some();
        sum = sum.add(crate::measurements::TokenUsage {
            input_tokens: Some(
                input
                    .saturating_add(read.unwrap_or(0))
                    .saturating_add(write.unwrap_or(0)),
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
    let model = format!("asb/{}", context.config.model);
    CommandSpec {
        program: "opencode".into(),
        args: vec![
            "run".into(),
            "--dir".into(),
            context.config.workspace.clone(),
            "--model".into(),
            model,
            "--format".into(),
            "json".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let npm = match connection.interface {
        crate::endpoint::Interface::AnthropicMessages => "@ai-sdk/anthropic",
        crate::endpoint::Interface::OpenAiChat => "@ai-sdk/openai-compatible",
        crate::endpoint::Interface::OpenAiResponses => "@ai-sdk/openai",
        _ => return Err(context.unsupported_endpoint("opencode")),
    };
    context
        .command
        .env(
            "ASB_AGENT_API_KEY",
            connection.credential.as_deref().unwrap_or("asb-local"),
        )
        .env(
            "OPENCODE_CONFIG_CONTENT",
            config(context.model, &connection.base_url, npm)?,
        );
    Ok(())
}

fn config(model: &str, base_url: &str, npm: &str) -> Result<String, Error> {
    let mut models = serde_json::Map::new();
    models.insert(model.to_owned(), serde_json::json!({"name": model}));
    serde_json::to_string(&serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {"asb": {
            "npm": npm, "name": "ASB provider",
            "options": {"baseURL": base_url, "apiKey": "{env:ASB_AGENT_API_KEY}"},
            "models": models
        }}
    }))
    .map_err(|error| Error::Run(format!("cannot configure OpenCode provider: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_opencode_adapter() {
        let config = config(
            "test-model",
            "http://127.0.0.1:3456/v1",
            "@ai-sdk/anthropic",
        )
        .expect("configuration succeeds");
        assert!(config.contains("test-model"));
        assert!(config.contains("ASB_AGENT_API_KEY"));
    }
}
