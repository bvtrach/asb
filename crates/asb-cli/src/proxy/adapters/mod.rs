//! Registry of proxy implementations.

pub(crate) mod llmtape;

use super::ProxyAdapter;

pub(super) static REGISTRY: &[&dyn ProxyAdapter] = &[&llmtape::ADAPTER];
