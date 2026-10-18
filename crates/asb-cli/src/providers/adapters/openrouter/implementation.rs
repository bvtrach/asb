//! OpenRouter-specific process configuration.

use crate::{Error, providers};
use std::process::Command;

pub(in crate::providers) fn prepare(
    command: &mut Command,
    model: &str,
    tape_base_url: Option<&str>,
    replay: bool,
) -> Result<(), Error> {
    let key = if replay {
        "asb-replay".to_owned()
    } else {
        providers::credential("openrouter").ok_or_else(|| {
            Error::Run("openrouter provider requires OPENROUTER_API_KEY or ~/.api_key_openrouter; set one before a live or record run".into())
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

pub(in crate::providers) fn configure_openai_compatible(
    command: &mut Command,
    model: &str,
    key: &str,
    base_url: &str,
) {
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
