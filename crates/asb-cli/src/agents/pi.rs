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
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "pi".into(),
        args: vec![
            "--provider".into(),
            context.config.provider.clone(),
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
    let cli2key = context.provider.eq_ignore_ascii_case("cli2key");
    let openrouter = context.provider.eq_ignore_ascii_case("openrouter");
    if !cli2key && !openrouter {
        return Ok(());
    }
    let directory = context.tools_dir.join("pi/config");
    let (provider, api, key_name) = if cli2key {
        ("cli2key", "openai-responses", "CLI2KEY_API_KEY")
    } else {
        ("openrouter", "openai-completions", "OPENROUTER_API_KEY")
    };
    let base_url = context
        .cli2key
        .map(|(_, url)| url.to_owned())
        .unwrap_or_else(|| {
            std::env::var("OPENROUTER_BASE_URL")
                .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into())
        });
    configure_provider(
        context.command,
        &directory,
        provider,
        context.model,
        &base_url,
        api,
        key_name,
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
