//! Legacy local pseudo-provider: retained for compatibility, not discoverable.

use super::{ProviderSpec, tape_endpoint_envs};

pub(super) fn spec(name: &str) -> Option<ProviderSpec> {
    name.eq_ignore_ascii_case("local").then(|| ProviderSpec {
        name: "local",
        credential: None,
        endpoint: None,
        model_env: None,
        tape_endpoint_envs: tape_endpoint_envs(),
        bridge_tool: None,
    })
}
