use super::{PromptAdapter, WorkloadAdapter};

/// One adapter covers the bundled prompt family. Its matching and materialized
/// fixture behavior are generic over the configured prompt identifier.
pub(super) static ADAPTER: PromptAdapter = PromptAdapter;

#[allow(dead_code)]
fn _assert_adapter_contract() {
    let _: &dyn WorkloadAdapter = &ADAPTER;
}
