use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::Error;
use crate::install::Recipe;
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "claude-code",
    "claude",
    Recipe::Npm {
        package: "@anthropic-ai/claude-code@2.1.301",
        executable: "claude",
    },
    command,
    prepare_adapter,
);
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
    // Keep Claude Code's user-level settings (which may pin a local provider
    // endpoint) out of a project-local benchmark.  The adapter owns this
    // isolated config root so tape and live runs use the endpoint prepared by
    // the runner rather than ambient host configuration.
    let config_root = context.runtime_dir.join("claude-code/config");
    std::fs::create_dir_all(&config_root).map_err(|error| {
        Error::Run(format!(
            "cannot create Claude Code project config directory {}: {error}",
            config_root.display()
        ))
    })?;
    context.command.env("CLAUDE_CONFIG_DIR", &config_root);
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
