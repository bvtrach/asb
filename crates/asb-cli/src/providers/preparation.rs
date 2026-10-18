//! Provider-owned pre-run environment preparation.

use crate::Error;
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
    bridge: Option<(&str, &str)>,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    if let Some(adapter) = super::adapters::find(provider) {
        adapter.prepare(command, model, bridge, tape_base_url, replay)?;
    }
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
        super::super::adapters::openrouter::implementation::configure_openai_compatible(
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
