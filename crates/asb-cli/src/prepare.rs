//! Pre-run environment preparation hooks.

use crate::Error;
use std::process::Command;

/// Apply provider-specific preparation immediately before launching an agent.
pub fn apply(
    command: &mut Command,
    agent: &str,
    provider: &str,
    model: &str,
    codex_executable: Option<&str>,
    cli2key: Option<(&str, &str)>,
) -> Result<(), Error> {
    if provider.eq_ignore_ascii_case("cli2key") {
        prepare_cli2key(command, agent, model, codex_executable, cli2key)?;
    }
    Ok(())
}

fn prepare_cli2key(
    command: &mut Command,
    agent: &str,
    model: &str,
    codex_executable: Option<&str>,
    cli2key: Option<(&str, &str)>,
) -> Result<(), Error> {
    let (key, base_url) = cli2key.ok_or_else(|| {
        Error::Run("cli2key provider was selected without an API endpoint".into())
    })?;
    command
        .env("CLI2KEY_API_KEY", key)
        .env("CLI2KEY_BASE_URL", base_url)
        .env("CLI2KEY_MODEL", model)
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", base_url)
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", base_url.trim_end_matches("/v1"));
    if agent.eq_ignore_ascii_case("opencode") {
        command.env("OPENCODE_CONFIG_CONTENT", opencode_config(model, base_url)?);
    }
    if let Some(executable) = codex_executable {
        command.env("CLI2KEY_CODEX_BIN", executable);
    }
    Ok(())
}

fn opencode_config(model: &str, base_url: &str) -> Result<String, Error> {
    let mut models = serde_json::Map::new();
    models.insert(model.to_owned(), serde_json::json!({"name": model}));
    serde_json::to_string(&serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "provider": {
            "cli2key": {
                "npm": "@ai-sdk/anthropic",
                "name": "cli2key (Codex Bridge)",
                "options": {
                    "baseURL": base_url,
                    "apiKey": "{env:CLI2KEY_API_KEY}"
                },
                "models": models
            }
        }
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
    fn cli2key_preparation_configures_the_selected_agent_api() {
        let mut command = Command::new("cli2key");
        apply(
            &mut command,
            "opencode",
            "cli2key",
            "gpt-test",
            Some("/project/codex"),
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
        )
        .expect("preparation succeeds");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert!(values.contains_key("CLI2KEY_API_KEY"));
        assert_eq!(
            values.get("CLI2KEY_MODEL").map(|value| value.as_ref()),
            Some("gpt-test")
        );
        assert_eq!(
            values.get("CLI2KEY_CODEX_BIN").map(|value| value.as_ref()),
            Some("/project/codex")
        );
        assert_eq!(
            values.get("OPENAI_API_KEY").map(|value| value.as_ref()),
            Some("cb_test")
        );
        assert_eq!(
            values.get("OPENAI_BASE_URL").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            values.get("ANTHROPIC_BASE_URL").map(|value| value.as_ref()),
            Some("http://127.0.0.1:3456")
        );
        let config: serde_json::Value = serde_json::from_str(
            values
                .get("OPENCODE_CONFIG_CONTENT")
                .expect("OpenCode config is set"),
        )
        .expect("OpenCode config is valid JSON");
        assert_eq!(
            config["provider"]["cli2key"]["models"]["gpt-test"]["name"],
            "gpt-test"
        );
        assert_eq!(
            config["provider"]["cli2key"]["options"]["apiKey"],
            "{env:CLI2KEY_API_KEY}"
        );
    }
}
