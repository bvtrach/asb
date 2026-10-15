//! Pre-run environment preparation hooks.

use crate::Error;
use std::io::Read;
use std::process::Command;

/// Apply provider-specific preparation immediately before launching an agent.
pub fn apply(
    command: &mut Command,
    provider: &str,
    model: &str,
    codex_executable: Option<&str>,
) -> Result<(), Error> {
    if provider.eq_ignore_ascii_case("cli2key") {
        prepare_cli2key(command, model, codex_executable)?;
    }
    Ok(())
}

fn prepare_cli2key(
    command: &mut Command,
    model: &str,
    codex_executable: Option<&str>,
) -> Result<(), Error> {
    let key = match std::env::var("CLI2KEY_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        Ok(_) => generate_key()?,
        Err(_) => generate_key()?,
    };
    command
        .env("CLI2KEY_API_KEY", &key)
        .env("CLI2KEY_MODEL", model);
    if let Some(endpoint) = std::env::var_os("CLI2KEY_BASE_URL") {
        let endpoint = endpoint.to_string_lossy();
        if endpoint.trim().is_empty() || endpoint.contains(['\r', '\n']) {
            return Err(Error::Run(
                "CLI2KEY_BASE_URL must be a non-empty single-line endpoint".into(),
            ));
        }
        command.env("CLI2KEY_BASE_URL", endpoint.as_ref());
    }
    if let Some(executable) = codex_executable {
        command.env("CLI2KEY_CODEX_BIN", executable);
    }
    Ok(())
}

fn generate_key() -> Result<String, Error> {
    let mut bytes = [0_u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| Error::Run(format!("cannot generate ephemeral cli2key: {error}")))?;
    let mut key = String::from("sk-asb-");
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut key, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_is_ephemeral_format() {
        let key = generate_key().expect("random source available");
        assert!(key.starts_with("sk-asb-"));
        assert_eq!(key.len(), 71);
        assert!(key[7..].bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn cli2key_preparation_targets_the_wrapper_not_codex() {
        let mut command = Command::new("cli2key");
        apply(&mut command, "cli2key", "gpt-test", Some("/project/codex"))
            .expect("preparation succeeds");
        let values = command
            .get_envs()
            .filter_map(|(name, value)| {
                value.map(|value| (name.to_string_lossy(), value.to_string_lossy()))
            })
            .collect::<std::collections::HashMap<_, _>>();
        assert!(values.contains_key("CLI2KEY_API_KEY"));
        assert_eq!(
            values.get("CLI2KEY_MODEL").map(|value| value.as_ref()),
            Some("gpt-test")
        );
        assert_eq!(
            values.get("CLI2KEY_CODEX_BIN").map(|value| value.as_ref()),
            Some("/project/codex")
        );
        assert!(!values.contains_key("OPENAI_API_KEY"));
        assert!(!values.contains_key("OPENAI_BASE_URL"));
    }
}
