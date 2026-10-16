use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "opencode",
    "opencode",
    Recipe::Npm {
        package: "opencode-ai@1.19.0",
        executable: "opencode",
    },
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    let model = if context.config.model.contains('/') {
        context.config.model.clone()
    } else if context.config.provider.eq_ignore_ascii_case("cli2key") {
        format!("cli2key/{}", context.config.model)
    } else if context.config.provider.eq_ignore_ascii_case("opencode-zen") {
        format!("opencode/{}", context.config.model)
    } else {
        context.config.model.clone()
    };
    CommandSpec {
        program: "opencode".into(),
        args: vec![
            "run".into(),
            "--model".into(),
            model,
            "--format".into(),
            "json".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if !context.provider.eq_ignore_ascii_case("cli2key") {
        return Ok(());
    }
    let (_, url) = context
        .cli2key
        .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
    context
        .command
        .env("OPENCODE_CONFIG_CONTENT", config(context.model, url)?);
    Ok(())
}

fn config(model: &str, base_url: &str) -> Result<String, Error> {
    let mut models = serde_json::Map::new();
    models.insert(model.to_owned(), serde_json::json!({"name": model}));
    serde_json::to_string(&serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {"cli2key": {
            "npm": "@ai-sdk/anthropic", "name": "cli2key (Codex Bridge)",
            "options": {"baseURL": base_url, "apiKey": "{env:CLI2KEY_API_KEY}"},
            "models": models
        }}
    }))
    .map_err(|error| {
        Error::Run(format!(
            "cannot configure OpenCode cli2key provider: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_opencode_adapter() {
        let config =
            config("test-model", "http://127.0.0.1:3456/v1").expect("configuration succeeds");
        assert!(config.contains("test-model"));
        assert!(config.contains("CLI2KEY_API_KEY"));
    }
}
