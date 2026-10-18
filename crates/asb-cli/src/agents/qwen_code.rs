use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "qwen-code",
    "qwen",
    Recipe::Npm {
        package: "@qwen-code/qwen-code@0.25.1",
        executable: "qwen",
    },
    &[crate::endpoint::CHAT, crate::endpoint::MESSAGES_KEY],
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "qwen".into(),
        args: vec![
            "--model".into(),
            context.config.model.clone(),
            "--output-format".into(),
            "json".into(),
            "--yolo".into(),
        ],
        append_prompt: true,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let connection = context.connection;
    let key = connection.credential.as_deref().unwrap_or("asb-local");
    match connection.interface {
        crate::endpoint::Interface::AnthropicMessages => {
            context
                .command
                .env("QWEN_DEFAULT_AUTH_TYPE", "anthropic")
                .env("ANTHROPIC_API_KEY", key)
                .env(
                    "ANTHROPIC_BASE_URL",
                    connection.base_url.trim_end_matches("/v1"),
                )
                .env("ANTHROPIC_MODEL", context.model)
                .env_remove("OPENAI_API_KEY")
                .env_remove("OPENAI_BASE_URL")
                .env_remove("OPENAI_API_BASE")
                .env_remove("OPENAI_MODEL");
        }
        crate::endpoint::Interface::OpenAiChat => {
            context
                .command
                .env("QWEN_DEFAULT_AUTH_TYPE", "openai")
                .env("OPENAI_API_KEY", key)
                .env("OPENAI_BASE_URL", &connection.base_url)
                .env("OPENAI_MODEL", context.model)
                .env_remove("ANTHROPIC_API_KEY")
                .env_remove("ANTHROPIC_BASE_URL");
        }
        _ => unreachable!("unsupported Qwen endpoint"),
    }
    Ok(())
}
