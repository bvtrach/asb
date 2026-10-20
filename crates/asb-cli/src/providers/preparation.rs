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
