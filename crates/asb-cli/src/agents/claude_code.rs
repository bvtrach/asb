use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::Error;
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("claude-code", "claude", command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "claude".into(),
        args: vec![
            "-p".into(),
            "--bare".into(),
            "--no-session-persistence".into(),
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

fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if context.provider.eq_ignore_ascii_case("cli2key") {
        let (key, url) = context
            .cli2key
            .ok_or_else(|| Error::Run("cli2key provider has no API endpoint".into()))?;
        context
            .command
            .env("ANTHROPIC_BASE_URL", url)
            .env("ANTHROPIC_API_KEY", key)
            .env("ANTHROPIC_AUTH_TOKEN", key)
            .env_remove("CLAUDE_CODE_USE_BEDROCK")
            .env_remove("CLAUDE_CODE_USE_VERTEX");
    }
    Ok(())
}
