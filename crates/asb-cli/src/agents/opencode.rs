use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, prepare};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("opencode", "opencode", command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    let model = if context.config.model.contains('/') {
        context.config.model.clone()
    } else if context.config.provider.eq_ignore_ascii_case("cli2key") {
        format!("cli2key/{}", context.config.model)
    } else if context.config.provider.eq_ignore_ascii_case("opencode-zen") {
        format!("opencode/{}", context.config.model)
    } else {
        context.config.model.clone()
    };
    CommandSpec {
        program: "opencode".into(),
        args: vec![
            "run".into(),
            "--model".into(),
            model,
            "--format".into(),
            "json".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if !context.provider.eq_ignore_ascii_case("cli2key") {
        return Ok(());
    }
    let (_, url) = context
        .cli2key
        .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
    context.command.env(
        "OPENCODE_CONFIG_CONTENT",
        prepare::opencode_config(context.model, url)?,
    );
    Ok(())
}
