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
);
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
