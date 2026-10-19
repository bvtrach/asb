//! Adapter for cli2key.
pub(in crate::providers) mod implementation;
use super::*;

fn cli2key_offers(_: &str) -> Vec<Offer> {
    offers(
        Address::Bridge,
        &[(Responses, Bearer), (Messages, ApiKey), (Messages, Bearer)],
    )
}

adapter!(
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
    fn anthropic_input_includes_cache(&self) -> bool {
        true
    },
    fn prepare(
        &self,
        command: &mut std::process::Command,
        model: &str,
        bridge: Option<(&str, &str)>,
        _tape: Option<&str>,
        _replay: bool,
    ) -> Result<(), crate::Error> {
        implementation::prepare(command, model, bridge)
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
        implementation::Bridge::connect(executable.as_deref())
            .map(|session| Some(Box::new(session) as Box<dyn crate::providers::ProviderSession>))
    },
    fn discovery_session(
        &self,
    ) -> Result<Option<Box<dyn crate::providers::ProviderSession>>, crate::Error> {
        let executable = implementation::local_executable();
        implementation::Bridge::connect(executable.as_deref())
            .map(|session| Some(Box::new(session) as Box<dyn crate::providers::ProviderSession>))
    }
);
