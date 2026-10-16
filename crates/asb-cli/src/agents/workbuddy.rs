use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, prepare};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("workbuddy", "codebuddy", command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "codebuddy".into(),
        args: vec![
            "--print".into(),
            "--output-format".into(),
            "json".into(),
            "--model".into(),
            context.config.model.clone(),
            "-y".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if !context.provider.eq_ignore_ascii_case("cli2key") {
        return Ok(());
    }
    let (key, base_url) = context
        .cli2key
        .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
    let directory = context.tools_dir.join("workbuddy/config");
    prepare::configure_workbuddy(&directory, context.model, base_url)?;
    context
        .command
        .env("CODEBUDDY_API_KEY", key)
        .env("CODEBUDDY_AUTH_TOKEN", key)
        .env("CODEBUDDY_BASE_URL", base_url)
        .env("CODEBUDDY_MODEL", context.model)
        .env("CODEBUDDY_CONFIG_DIR", &directory)
        .env("CODEBUDDY_API_KEY_HELPER_DISABLED", "1");
    Ok(())
}
