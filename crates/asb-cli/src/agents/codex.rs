use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe, prepare};
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
        let directory = context.tools_dir.join("codex/config");
        prepare::configure_codex(&directory, context.model, url)?;
        context
            .command
            .env("CODEX_HOME", &directory)
            .env("CLI2KEY_API_KEY", key);
    }
    Ok(())
}
