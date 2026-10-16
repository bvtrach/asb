//! Provider-wide pre-run environment preparation.

use crate::{Error, providers};
use std::process::Command;

/// Apply only provider-wide configuration to a command.
///
/// Agent adapters are invoked separately by the runner after this baseline has
/// been established. Keeping this module target-neutral prevents a provider
/// change from silently changing another adapter's configuration.
pub fn apply_with_tape(
    command: &mut Command,
    provider: &str,
    model: &str,
    cli2key: Option<(&str, &str)>,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    if provider.eq_ignore_ascii_case("cli2key") {
        prepare_cli2key(command, model, cli2key)?;
    } else if provider.eq_ignore_ascii_case("openrouter") {
        prepare_openrouter(command, model, tape_base_url, replay)?;
    }
    Ok(())
}

fn prepare_openrouter(
    command: &mut Command,
    model: &str,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    let key = if replay {
        "asb-replay".to_owned()
    } else {
        providers::credential("openrouter").ok_or_else(|| {
            Error::Run(
                "openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter; set one before a live or record run".into(),
            )
        })?
    };
    let base_url = tape_base_url
        .map(str::to_owned)
        .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
        .unwrap_or_else(|| "https://openrouter.ai/api/v1".into());
    configure_openai_compatible(command, model, &key, &base_url);
    command.env("OPENROUTER_API_KEY", &key);
    Ok(())
}

fn configure_openai_compatible(command: &mut Command, model: &str, key: &str, base_url: &str) {
    let root = base_url.trim_end_matches('/');
    let api_root = root.strip_suffix("/v1").unwrap_or(root);
    command
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", root)
        .env("OPENAI_API_BASE", root)
        .env("OPENAI_MODEL", model)
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", api_root);
}

fn prepare_cli2key(
    command: &mut Command,
    model: &str,
    cli2key: Option<(&str, &str)>,
) -> Result<(), Error> {
    let (key, base_url) = cli2key.ok_or_else(|| {
        Error::Run(
            "cli2key provider has no API endpoint; run `asb tool install cli2key` or set CLI2KEY_BASE_URL and CLI2KEY_API_KEY".into(),
        )
    })?;
    command
        .env("CLI2KEY_API_KEY", key)
        .env("CLI2KEY_BASE_URL", base_url)
        .env("CLI2KEY_MODEL", model)
        .env("OPENAI_API_KEY", key)
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_API_BASE", base_url)
        .env("OPENAI_MODEL", model)
        .env("ANTHROPIC_MODEL", model)
        .env("ANTHROPIC_API_KEY", key)
        .env("ANTHROPIC_BASE_URL", base_url.trim_end_matches("/v1"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(command: &Command) -> std::collections::HashMap<String, String> {
        command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    (
                        name.to_string_lossy().into_owned(),
                        value.to_string_lossy().into_owned(),
                    )
                })
            })
            .collect()
    }

    #[test]
    fn cli2key_preparation_sets_protocol_compatible_environment() {
        let mut command = Command::new("bridge-client");
        apply_with_tape(
            &mut command,
            "cli2key",
            "gpt-test",
            Some(("cb_test", "http://127.0.0.1:3456/v1")),
            None,
            false,
        )
        .expect("preparation succeeds");
        let values = values(&command);
        assert_eq!(values.get("CLI2KEY_MODEL"), Some(&"gpt-test".to_owned()));
        assert_eq!(values.get("OPENAI_API_KEY"), Some(&"cb_test".to_owned()));
        assert_eq!(
            values.get("ANTHROPIC_BASE_URL"),
            Some(&"http://127.0.0.1:3456".to_owned())
        );
    }

    #[test]
    fn openrouter_baseline_uses_compatible_protocol_endpoints() {
        let mut command = Command::new("bridge-client");
        configure_openai_compatible(
            &mut command,
            "example/model",
            "or-test",
            "https://openrouter.ai/api/v1",
        );
        let values = values(&command);
        assert_eq!(
            values.get("OPENAI_BASE_URL"),
            Some(&"https://openrouter.ai/api/v1".to_owned())
        );
        assert_eq!(
            values.get("ANTHROPIC_BASE_URL"),
            Some(&"https://openrouter.ai/api".to_owned())
        );
    }

    #[test]
    fn replay_openrouter_does_not_need_a_live_credential() {
        let mut command = Command::new("bridge-client");
        apply_with_tape(
            &mut command,
            "openrouter",
            "example/model",
            None,
            Some("http://127.0.0.1:4567/v1"),
            true,
        )
        .expect("replay preparation succeeds");
        assert_eq!(
            values(&command).get("OPENROUTER_API_KEY"),
            Some(&"asb-replay".to_owned())
        );
    }
}
