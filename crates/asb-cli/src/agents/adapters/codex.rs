use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "codex",
    "codex",
    Recipe::Npm {
        package: "@openai/codex@0.150.1",
        executable: "codex",
    },
    &[crate::endpoint::RESPONSES],
    command,
    prepare_adapter,
)
.with_state_environment_vars(&["CODEX_HOME"]);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "codex".into(),
        args: vec![
            "exec".into(),
            "--skip-git-repo-check".into(),
            "--ephemeral".into(),
            "--ignore-rules".into(),
            "--disable".into(),
            "multi_agent".into(),
            "--disable".into(),
            "apps".into(),
            "--disable".into(),
            "plugins".into(),
            "--approve-for-me".into(),
            "--json".into(),
            "--model".into(),
            context.config.model.clone(),
        ],
        append_prompt: true,
    }
}

fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let directory = context.runtime_dir.join("codex/config");
    configure(
        &directory,
        context.model,
        &connection.base_url,
        connection.credential.is_some(),
    )?;
    context.command.env("CODEX_HOME", &directory);
    if let Some(key) = connection.credential.as_deref() {
        context.command.env("ASB_AGENT_API_KEY", key);
    } else {
        context.command.env_remove("ASB_AGENT_API_KEY");
    }
    Ok(())
}

fn configure(
    directory: &std::path::Path,
    model: &str,
    base_url: &str,
    authenticated: bool,
) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Codex provider configuration directory: {error}"
        ))
    })?;
    let model = toml::Value::String(model.to_owned());
    let base_url = toml::Value::String(base_url.to_owned());
    let key = if authenticated {
        "env_key = \"ASB_AGENT_API_KEY\"\n"
    } else {
        ""
    };
    let configuration = format!(
        "model = {model}\nmodel_provider = \"asb_provider\"\nweb_search = \"disabled\"\n\n[features]\napps = false\nplugins = false\nenable_mcp_apps = false\n\n[model_providers.asb_provider]\nname = \"ASB provider\"\nbase_url = {base_url}\n{key}wire_api = \"responses\"\nrequires_openai_auth = false\n"
    );
    std::fs::write(directory.join("config.toml"), configuration).map_err(|error| {
        Error::Run(format!(
            "cannot write Codex provider configuration: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_configuration_uses_safe_values_and_generic_credentials() {
        let directory =
            std::env::temp_dir().join(format!("asb-codex-config-{}", std::process::id()));
        configure(&directory, "model\"name", "http://127.0.0.1:1234/v1", true).unwrap();
        let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
        let config: toml::Value = toml::from_str(&contents).unwrap();
        assert_eq!(config["model"].as_str(), Some("model\"name"));
        assert_eq!(config["model_provider"].as_str(), Some("asb_provider"));
        assert_eq!(
            config["model_providers"]["asb_provider"]["base_url"].as_str(),
            Some("http://127.0.0.1:1234/v1")
        );
        assert_eq!(
            config["model_providers"]["asb_provider"]["wire_api"].as_str(),
            Some("responses")
        );
        assert_eq!(
            config["model_providers"]["asb_provider"]["env_key"].as_str(),
            Some("ASB_AGENT_API_KEY")
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
