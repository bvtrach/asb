use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "openhands",
    "openhands",
    Recipe::Python {
        package: "openhands==1.16.0",
        dependencies: &[],
        executable: "openhands",
    },
    &[crate::endpoint::CHAT, crate::endpoint::MESSAGES_KEY],
    command,
    prepare_adapter,
)
.with_replay_credential_vars(&["LLM_API_KEY"]);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "openhands".into(),
        args: vec![
            "--headless".into(),
            "--json".into(),
            "--yolo".into(),
            "--override-with-envs".into(),
            "--task".into(),
            context.prompt.into(),
        ],
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let key = connection.credential.as_deref().unwrap_or("asb-local");
    let (prefix, url) = match connection.interface {
        crate::endpoint::Interface::AnthropicMessages => {
            ("anthropic", connection.base_url.trim_end_matches("/v1"))
        }
        crate::endpoint::Interface::OpenAiChat => ("openai", connection.base_url.as_str()),
        _ => unreachable!("unsupported OpenHands endpoint"),
    };
    context
        .command
        .env("LLM_API_KEY", key)
        .env("LLM_BASE_URL", url)
        .env("LLM_MODEL", format!("{prefix}/{}", context.model))
        .env("OPENHANDS_SUPPRESS_BANNER", "1");
    Ok(())
}
