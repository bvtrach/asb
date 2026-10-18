use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "continue",
    "cn",
    Recipe::Npm {
        package: "@continuedev/cli@1.5.47",
        executable: "cn",
    },
    &[crate::endpoint::CHAT, crate::endpoint::MESSAGES_KEY],
    command,
    prepare_adapter,
)
.with_replay_credential_vars(&["CONTINUE_API_KEY"]);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "cn".into(),
        args: vec![
            "-p".into(),
            context.prompt.into(),
            "--model".into(),
            context.config.model.clone(),
            "--format".into(),
            "json".into(),
            "--auto".into(),
        ],
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let directory = context.runtime_dir.join("continue/config");
    let provider = match connection.interface {
        crate::endpoint::Interface::AnthropicMessages => "anthropic",
        crate::endpoint::Interface::OpenAiChat => "openai",
        _ => unreachable!("unsupported Continue endpoint"),
    };
    let key = connection.credential.as_deref().unwrap_or("asb-local");
    let url = &connection.base_url;
    configure(
        &directory,
        provider,
        context.model,
        url,
        "ASB_AGENT_API_KEY",
    )?;
    context
        .command
        .env("CONTINUE_CONFIG", directory.join("config.yaml"))
        .env("CONTINUE_GLOBAL_DIR", &directory)
        .env("CONTINUE_API_KEY", key)
        .env("ASB_AGENT_API_KEY", key)
        .env("CONTINUE_API_BASE", url)
        .env("CONTINUE_MODEL", context.model);
    Ok(())
}

fn configure(
    directory: &std::path::Path,
    provider: &str,
    model: &str,
    base_url: &str,
    api_key_env: &str,
) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Continue provider configuration directory: {error}"
        ))
    })?;
    let model = serde_json::to_string(model)
        .map_err(|error| Error::Run(format!("cannot render Continue model: {error}")))?;
    let base_url = serde_json::to_string(base_url)
        .map_err(|error| Error::Run(format!("cannot render Continue endpoint: {error}")))?;
    let configuration = format!(
        "name: ASB provider\nversion: 1.0.0\nmodels:\n  - name: {model}\n    provider: {provider}\n    model: {model}\n    apiBase: {base_url}\n    apiKey: ${{{{ secrets.//{api_key_env} }}}}\n"
    );
    std::fs::write(directory.join("config.yaml"), configuration).map_err(|error| {
        Error::Run(format!(
            "cannot write Continue provider configuration: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_continue_adapter() {
        let directory =
            std::env::temp_dir().join(format!("asb-continue-config-{}", std::process::id()));
        configure(
            &directory,
            "anthropic",
            "test-model",
            "http://127.0.0.1:3456/v1",
            "CLI2KEY_API_KEY",
        )
        .expect("configuration succeeds");
        let text =
            std::fs::read_to_string(directory.join("config.yaml")).expect("configuration exists");
        assert!(text.contains("test-model"));
        assert!(text.contains("CLI2KEY_API_KEY"));
        let _ = std::fs::remove_dir_all(directory);
    }
}
