use super::{CommandContext, CommandSpec, PreparationContext, StaticAdapter};
use crate::{Error, install::Recipe, providers};
pub(super) static ADAPTER: StaticAdapter =
    StaticAdapter::new("goose", "goose", Recipe::Goose, command, prepare_adapter);
fn command(context: &CommandContext<'_>) -> CommandSpec {
    CommandSpec {
        program: "goose".into(),
        args: vec![
            "run".into(),
            "--no-session".into(),
            "--max-turns".into(),
            "1".into(),
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
            let url = context.tape.map_or(url, |(_, endpoint)| endpoint);
            let key = if context
                .tape
                .is_some_and(|(mode, _)| mode == crate::tape::Mode::Replay)
            {
                "asb-replay"
            } else {
                key
            };
            context
                .command
                .env("GOOSE_PROVIDER", "anthropic")
                .env("GOOSE_MODEL", context.model)
                .env("ANTHROPIC_HOST", url.trim_end_matches("/v1"))
                .env("ANTHROPIC_API_KEY", key)
                .env("ANTHROPIC_AUTH_TOKEN", key);
        }
    } else if context.provider.eq_ignore_ascii_case("openrouter") {
        let key = if context
            .tape
            .is_some_and(|(mode, _)| mode == crate::tape::Mode::Replay)
        {
            "asb-replay".to_owned()
        } else {
            providers::credential("openrouter").ok_or_else(|| {
                Error::Run(
                    "openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter"
                        .into(),
                )
            })?
        };
        let base_url = context
            .tape
            .map(|(_, url)| url.to_owned())
            .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
            .unwrap_or_else(|| "https://openrouter.ai/api/v1".into());
        context
            .command
            .env("GOOSE_PROVIDER", "openai")
            .env("GOOSE_MODEL", context.model)
            .env("OPENAI_HOST", base_url.trim_end_matches("/v1"))
            .env("OPENAI_BASE_PATH", "/v1/chat/completions")
            .env("OPENAI_API_KEY", key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use std::process::Command;

    fn environment(command: &Command) -> HashMap<String, String> {
        command
            .get_envs()
            .filter_map(|(key, value)| {
                value.map(|value| {
                    (
                        key.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn openrouter_replay_uses_tape_and_dummy_credential() {
        let mut command = Command::new("goose");
        prepare_adapter(&mut PreparationContext {
            command: &mut command,
            provider: "openrouter",
            model: "test-model",
            cli2key: None,
            tools_dir: Path::new("."),
            runtime_dir: Path::new("."),
            tape: Some((crate::tape::Mode::Replay, "http://127.0.0.1:4321/v1")),
        })
        .unwrap();
        let env = environment(&command);
        assert_eq!(env["GOOSE_PROVIDER"], "openai");
        assert_eq!(env["OPENAI_HOST"], "http://127.0.0.1:4321");
        assert_eq!(env["OPENAI_BASE_PATH"], "/v1/chat/completions");
        assert_eq!(env["OPENAI_API_KEY"], "asb-replay");
    }

    #[test]
    fn cli2key_tape_endpoint_wins_over_bridge_endpoint() {
        for mode in [crate::tape::Mode::Record, crate::tape::Mode::Replay] {
            let mut command = Command::new("goose");
            prepare_adapter(&mut PreparationContext {
                command: &mut command,
                provider: "cli2key",
                model: "test-model",
                cli2key: Some(("fixture-key", "http://127.0.0.1:9000/v1")),
                tools_dir: Path::new("."),
                runtime_dir: Path::new("."),
                tape: Some((mode, "http://127.0.0.1:4321/v1")),
            })
            .unwrap();
            let env = environment(&command);
            assert_eq!(env["GOOSE_PROVIDER"], "anthropic");
            assert_eq!(env["ANTHROPIC_HOST"], "http://127.0.0.1:4321");
            assert_eq!(
                env["ANTHROPIC_API_KEY"],
                if mode == crate::tape::Mode::Replay {
                    "asb-replay"
                } else {
                    "fixture-key"
                }
            );
        }
    }
}
