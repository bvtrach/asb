use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, prepare};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new("pi", "pi", command, prepare_adapter);
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
    let directory = context.pi_config_dir.ok_or_else(|| {
        Error::Run(
            "cannot configure Pi without a project-local provider configuration directory".into(),
        )
    })?;
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
    prepare::configure_pi_provider(
        context.command,
        directory,
        provider,
        context.model,
        &base_url,
        api,
        key_name,
    )
}
