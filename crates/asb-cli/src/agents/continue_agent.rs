use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe, prepare};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "continue",
    "cn",
    Recipe::Npm {
        package: "@continuedev/cli@1.5.47",
        executable: "cn",
    },
    command,
    prepare_adapter,
);
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
    if !context.provider.eq_ignore_ascii_case("cli2key")
        && !context.provider.eq_ignore_ascii_case("openrouter")
    {
        return Ok(());
    }
    let directory = context.tools_dir.join("continue/config");
    let url = context
        .cli2key
        .map(|(_, url)| url.to_owned())
        .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
        .unwrap_or_else(|| "https://openrouter.ai/api/v1".into());
    let (provider, key_name) = if context.provider.eq_ignore_ascii_case("cli2key") {
        ("anthropic", "CLI2KEY_API_KEY")
    } else {
        ("openai", "OPENROUTER_API_KEY")
    };
    prepare::configure_continue(&directory, provider, context.model, &url, key_name)?;
    context
        .command
        .env("CONTINUE_CONFIG", directory.join("config.yaml"))
        .env("CONTINUE_GLOBAL_DIR", &directory);
    Ok(())
}
