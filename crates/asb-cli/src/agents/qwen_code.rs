use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::Error;
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("qwen-code", "qwen", command, prepare_adapter);
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
            .env_remove("OPENAI_API_KEY")
            .env_remove("OPENAI_BASE_URL")
            .env_remove("OPENAI_API_BASE")
            .env_remove("OPENAI_MODEL");
    }
    Ok(())
}
