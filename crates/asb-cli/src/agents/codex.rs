use super::{CommandContext, CommandSpec, StaticAdapter, no_prepare};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("codex", "codex", command, no_prepare);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "codex".into(),
        args: vec![
            "exec".into(),
            "--skip-git-repo-check".into(),
            "--json".into(),
            "--model".into(),
            context.config.model.clone(),
        ],
        append_prompt: true,
    }
}
