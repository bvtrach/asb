use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::Error;
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("goose", "goose", command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "goose".into(),
        args: vec![
            "run".into(),
            "--no-session".into(),
            "--output-format".into(),
            "json".into(),
            "-t".into(),
            context.prompt.into(),
        ],
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if context.provider.eq_ignore_ascii_case("cli2key") {
        if let Some((key, url)) = context.cli2key {
            context
                .command
                .env("GOOSE_PROVIDER", "openai")
                .env("GOOSE_MODEL", context.model)
                .env("OPENAI_HOST", url.trim_end_matches("/v1"))
                .env("OPENAI_BASE_PATH", "/v1/chat/completions")
                .env("OPENAI_API_KEY", key);
        }
    }
    Ok(())
}
