use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::Error;
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("mini-swe", "mini", command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "mini".into(),
        args: vec![
            "--model".into(),
            format!("anthropic/{}", context.config.model),
            "--task".into(),
            context.prompt.into(),
            "--yolo".into(),
            "--exit-immediately".into(),
            "--output".into(),
            ".asb-mini-swe.traj.json".into(),
        ],
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if context.provider.eq_ignore_ascii_case("cli2key") {
        context
            .command
            .env("MSWEA_CONFIGURED", "true")
            .env("MSWEA_MODEL_NAME", format!("anthropic/{}", context.model))
            .env("MSWEA_SILENT_STARTUP", "1")
            .env("MSWEA_COST_TRACKING", "ignore_errors");
    }
    Ok(())
}
