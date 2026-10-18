use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "codex",
    "codex",
    Recipe::Npm {
        package: "@openai/codex@0.162.1",
        executable: "codex",
    },
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "codex".into(),
        args: vec![
            "exec".into(),
            "--skip-git-repo-check".into(),
            "--ephemeral".into(),
            "--ignore-rules".into(),
            "--json".into(),
            "--model".into(),
            context.config.model.clone(),
        ],
        append_prompt: true,
    }
}

fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if context.provider.eq_ignore_ascii_case("cli2key") {
        let (key, url) = context
            .cli2key
            .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
        let directory = context.runtime_dir.join("codex/config");
        configure(&directory, context.model, url)?;
        context
            .command
            .env("CODEX_HOME", &directory)
            .env("CLI2KEY_API_KEY", key);
        if let Some(executable) = super::resolve_from_tools("codex", context.tools_dir) {
            context.command.env("CLI2KEY_CODEX_BIN", executable);
        }
    } else if context.provider.eq_ignore_ascii_case("ollama") {
        let base_url = if let Some((_, url)) = context.tape {
            url.to_owned()
        } else {
            let tags_url = crate::providers::model_spec("ollama")
                .and_then(|spec| spec.url())
                .ok_or_else(|| Error::Run("Ollama endpoint is not configured".into()))?;
            let root = tags_url
                .strip_suffix("/api/tags")
                .ok_or_else(|| Error::Run("Ollama model endpoint must end in /api/tags".into()))?;
            format!("{root}/v1")
        };
        let directory = context.runtime_dir.join("codex/config");
        configure_ollama(&directory, context.model, &base_url)?;
        context.command.env("CODEX_HOME", &directory);
    }
    Ok(())
}

fn configure_ollama(directory: &std::path::Path, model: &str, base_url: &str) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Codex provider configuration directory: {error}"
        ))
    })?;
    let model = toml::Value::String(model.to_owned());
    let base_url = toml::Value::String(base_url.to_owned());
    let configuration = format!(
        "model = {model}\nmodel_provider = \"asb_ollama\"\nweb_search = \"disabled\"\n\n[model_providers.asb_ollama]\nname = \"Ollama\"\nbase_url = {base_url}\nwire_api = \"responses\"\nrequires_openai_auth = false\n"
    );
    std::fs::write(directory.join("config.toml"), configuration).map_err(|error| {
        Error::Run(format!(
            "cannot write Codex provider configuration: {error}"
        ))
    })
}

fn configure(directory: &std::path::Path, model: &str, base_url: &str) -> Result<(), Error> {
    std::fs::create_dir_all(directory).map_err(|error| {
        Error::Run(format!(
            "cannot create Codex provider configuration directory: {error}"
        ))
    })?;
    let configuration = format!(
        "model = \"{model}\"\nmodel_provider = \"cli2key\"\n\n[model_providers.cli2key]\nname = \"CLI2KEY bridge\"\nbase_url = \"{base_url}\"\nenv_key = \"CLI2KEY_API_KEY\"\nwire_api = \"responses\"\nrequires_openai_auth = false\n"
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
    fn provider_configuration_is_owned_by_the_codex_adapter() {
        let directory =
            std::env::temp_dir().join(format!("asb-codex-config-{}", std::process::id()));
        configure(&directory, "test-model", "http://127.0.0.1:3456/v1")
            .expect("configuration succeeds");
        let text =
            std::fs::read_to_string(directory.join("config.toml")).expect("configuration exists");
        assert!(text.contains("test-model"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn ollama_configuration_uses_selected_endpoint_and_safe_toml_values() {
        let directory =
            std::env::temp_dir().join(format!("asb-codex-ollama-{}", std::process::id()));
        configure_ollama(&directory, "model\"name", "http://127.0.0.1:1234/v1").unwrap();
        let contents = std::fs::read_to_string(directory.join("config.toml")).unwrap();
        let config: toml::Value = toml::from_str(&contents).unwrap();
        assert_eq!(config["model"].as_str(), Some("model\"name"));
        assert_eq!(config["model_provider"].as_str(), Some("asb_ollama"));
        assert_eq!(config["web_search"].as_str(), Some("disabled"));
        assert_eq!(
            config["model_providers"]["asb_ollama"]["base_url"].as_str(),
            Some("http://127.0.0.1:1234/v1")
        );
        assert_eq!(
            config["model_providers"]["asb_ollama"]["wire_api"].as_str(),
            Some("responses")
        );
        let _ = std::fs::remove_dir_all(directory);
    }
}
