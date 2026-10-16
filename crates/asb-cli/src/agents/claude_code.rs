use super::{CommandContext, CommandSpec, StaticAdapter, no_prepare};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("claude-code", "claude", command, no_prepare);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "claude".into(),
        args: vec![
            "-p".into(),
            "--output-format".into(),
            "json".into(),
            "--model".into(),
            context.config.model.clone(),
            "--dangerously-skip-permissions".into(),
            context.prompt.into(),
        ],
        append_prompt: false,
    }
}
