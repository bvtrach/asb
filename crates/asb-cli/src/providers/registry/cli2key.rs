//! Adapter for cli2key.
use super::*;

fn cli2key_offers(_: &str) -> Vec<Offer> {
    offers(
        Address::Bridge,
        &[(Responses, Bearer), (Messages, ApiKey), (Messages, Bearer)],
    )
}

adapter!(
    cli2key,
    "cli2key",
    Some("CLI2KEY_API_KEY"),
    Some("CLI2KEY_BASE_URL"),
    Some("CLI2KEY_MODEL"),
    Some("cli2key"),
    &["CLI2KEY_BASE_URL"],
    Some("CLI2KEY_BASE_URL"),
    None,
    DataUnpaginated,
    "gpt-6.1-sol",
    cli2key_offers,
    fn prepare(
        &self,
        command: &mut std::process::Command,
        model: &str,
        bridge: Option<(&str, &str)>,
        _tape: Option<&str>,
        _replay: bool,
    ) -> Result<(), crate::Error> {
        crate::providers::cli2key::prepare(command, model, bridge)
    },
    fn known_models(&self) -> Option<&'static [&'static str]> {
        Some(&["gpt-6-astra", "gpt-6.1-sol", "gpt-6-luna"])
    },
    fn default_tool_support(&self) -> Option<bool> {
        Some(true)
    },
    fn start_session(
        &self,
        project_root: &std::path::Path,
        tools_dir: &str,
    ) -> Result<Option<Box<dyn crate::providers::ProviderSession>>, crate::Error> {
        let executable = crate::tools::resolve("cli2key", project_root, tools_dir);
        crate::providers::cli2key::Bridge::connect(executable.as_deref())
            .map(|session| Some(Box::new(session) as Box<dyn crate::providers::ProviderSession>))
    },
    fn discovery_session(
        &self,
    ) -> Result<Option<Box<dyn crate::providers::ProviderSession>>, crate::Error> {
        let executable = crate::providers::cli2key::local_executable();
        crate::providers::cli2key::Bridge::connect(executable.as_deref())
            .map(|session| Some(Box::new(session) as Box<dyn crate::providers::ProviderSession>))
    }
);
