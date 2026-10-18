use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe, providers};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "cline",
    "cline",
    Recipe::Npm {
        package: "cline@3.0.72",
        executable: "cline",
    },
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "cline".into(),
        args: vec![
            "--json".into(),
            "--auto-approve".into(),
            "true".into(),
            "--provider".into(),
            if context.config.provider.eq_ignore_ascii_case("openrouter") {
                "openrouter".into()
            } else {
                "openai-native".into()
            },
            "--model".into(),
            context.config.model.clone(),
            context.prompt.into(),
        ],
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let directory = context.runtime_dir.join("cline/config");
    if context.provider.eq_ignore_ascii_case("cli2key") {
        let (key, url) = context
            .cli2key
            .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
        configure(&directory, "openai-native", context.model, url, key)?;
        context
            .command
            .env("OPENAI_API_KEY", key)
            .env("OPENAI_BASE_URL", url)
            .env("OPENAI_API_BASE", url)
            .env("OPENAI_MODEL", context.model)
            .env("CLINE_API_KEY", key);
    } else if context.provider.eq_ignore_ascii_case("openrouter") {
        let url = context
            .tape
            .map(|(_, url)| url.to_owned())
            .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
            .unwrap_or_else(|| "https://openrouter.ai/api/v1".into());
        let key = if context
            .tape
            .is_some_and(|(mode, _)| mode == crate::tape::Mode::Replay)
        {
            "asb-replay".to_owned()
        } else {
            providers::credential("openrouter").ok_or_else(|| {
                Error::Run(
                    "openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter"
                        .into(),
                )
            })?
        };
        configure(&directory, "openrouter", context.model, &url, &key)?;
    } else {
        return Ok(());
    }
    context
        .command
        .env("CLINE_DIR", &directory)
        .env("CLINE_DATA_DIR", directory.join("data"))
        .env(
            "CLINE_PROVIDER_SETTINGS_PATH",
            directory.join("data/settings/providers.json"),
        )
        .env(
            "CLINE_GLOBAL_SETTINGS_PATH",
            directory.join("data/settings/global-settings.json"),
        );
    Ok(())
}

fn configure(
    directory: &std::path::Path,
    provider: &str,
    model: &str,
    base_url: &str,
    key: &str,
) -> Result<(), Error> {
    let settings_dir = directory.join("data/settings");
    std::fs::create_dir_all(&settings_dir).map_err(|error| {
        Error::Run(format!(
            "cannot create Cline provider configuration directory: {error}"
        ))
    })?;
    let configuration = serde_json::json!({
        "version": 1, "lastUsedProvider": provider,
        "providers": { provider: { "settings": {
            "provider": provider, "model": model, "baseUrl": base_url, "apiKey": key,
            "protocol": "openai-responses", "client": "openai"
        }, "updatedAt": "1970-01-01T00:00:00.000Z", "tokenSource": "manual"}}
    });
    let content = serde_json::to_vec_pretty(&configuration).map_err(|error| {
        Error::Run(format!(
            "cannot render Cline provider configuration: {error}"
        ))
    })?;
    std::fs::write(settings_dir.join("providers.json"), content).map_err(|error| {
        Error::Run(format!(
            "cannot write Cline provider configuration: {error}"
        ))
    })?;
    let global = serde_json::json!({
        "disabledTools": ["web_search", "image_generation"],
        "tools": {"web_search": {"enabled": false}, "image_generation": {"enabled": false}}
    });
    let global_content = serde_json::to_vec_pretty(&global)
        .map_err(|error| Error::Run(format!("cannot render Cline global settings: {error}")))?;
    std::fs::write(settings_dir.join("global-settings.json"), global_content)
        .map_err(|error| Error::Run(format!("cannot write Cline global settings: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_configuration_is_owned_by_the_cline_adapter() {
        let directory =
            std::env::temp_dir().join(format!("asb-cline-config-{}", std::process::id()));
        configure(
            &directory,
            "openai-native",
            "test-model",
            "http://127.0.0.1:3456/v1",
            "test-key",
        )
        .expect("configuration succeeds");
        let text = std::fs::read_to_string(directory.join("data/settings/providers.json"))
            .expect("configuration exists");
        assert!(text.contains("test-model"));
        let _ = std::fs::remove_dir_all(directory);
    }
}
