use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe};
pub(super) static ADAPTER: StaticAdapter = StaticAdapter::new(
    "openjiuwen",
    "openjiuwen",
    Recipe::Python {
        package: "openjiuwen[cli]==0.1.19.post2",
        executable: "openjiuwen",
    },
    command,
    prepare_adapter,
);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "openjiuwen".into(),
        args: [
            "--model",
            context.config.model.as_str(),
            "--provider",
            "anthropic",
            "--api-base",
            context.endpoint.unwrap_or_default(),
            "run",
            "-f",
            "json",
            context.prompt,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        append_prompt: false,
    }
}
fn prepare_adapter(context: &mut PreparationContext<'_>) -> Result<(), Error> {
    if context.provider.eq_ignore_ascii_case("cli2key") {
        if let Some((key, _)) = context.cli2key {
            context.command.env("OPENJIUWEN_API_KEY", key);
        }
    }
    Ok(())
}
