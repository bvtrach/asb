use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "qwen-code",
    "qwen",
    Recipe::Npm {
        package: "@qwen-code/qwen-code@0.25.1",
        executable: "qwen",
    },
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
    if context.provider.eq_ignore_ascii_case("cli2key") {
        context
            .command
            .env("QWEN_DEFAULT_AUTH_TYPE", "anthropic")
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_API_BASE")
            .env_remove("OPENAI_MODEL");
    }
    Ok(())
}
