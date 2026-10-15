//! Pre-run environment preparation hooks.

use crate::Error;
use std::io::Read;
use std::process::Command;

/// Apply provider-specific preparation immediately before launching an agent.
pub fn apply(command: &mut Command, provider: &str, model: &str) -> Result<(), Error> {
    if provider.eq_ignore_ascii_case("cli2key") {
        prepare_cli2key(command, model)?;
    }
    Ok(())
}

fn prepare_cli2key(command: &mut Command, model: &str) -> Result<(), Error> {
    let endpoint = std::env::var("CLI2KEY_BASE_URL")
        .map_err(|_| Error::Run("cli2key requires CLI2KEY_BASE_URL for its local bridge".into()))?;
    if endpoint.trim().is_empty() || endpoint.contains(['\r', '\n']) {
        return Err(Error::Run(
            "CLI2KEY_BASE_URL must be a non-empty single-line endpoint".into(),
        ));
    }
    let key = match std::env::var("CLI2KEY_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        Ok(_) => generate_key()?,
        Err(_) => generate_key()?,
    };
    command
        .env("CLI2KEY_API_KEY", &key)
        .env("CLI2KEY_BASE_URL", &endpoint)
        .env("CLI2KEY_MODEL", model)
        .env("OPENAI_API_KEY", &key)
        .env("OPENAI_BASE_URL", &endpoint);
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
}
