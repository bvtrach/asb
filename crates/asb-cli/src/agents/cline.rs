use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, prepare, providers};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("cline", "cline", command, prepare_adapter);
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
    let directory = context.tools_dir.join("cline/config");
    if context.provider.eq_ignore_ascii_case("cli2key") {
        let (key, url) = context
            .cli2key
            .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
        prepare::configure_cline(&directory, "openai-native", context.model, url, key)?;
        context
            .command
            .env("OPENAI_API_KEY", key)
            .env("OPENAI_BASE_URL", url)
            .env("OPENAI_API_BASE", url)
            .env("OPENAI_MODEL", context.model)
            .env("CLINE_API_KEY", key);
    } else if context.provider.eq_ignore_ascii_case("openrouter") {
        let url = std::env::var("OPENROUTER_BASE_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1".into());
        let key = providers::credential("openrouter").ok_or_else(|| {
            Error::Run(
                "openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter".into(),
            )
        })?;
        prepare::configure_cline(&directory, "openrouter", context.model, &url, &key)?;
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
