//! Adapters for externally sourced benchmark suites.
//!
//! Each supported suite has a module so suite-specific setup, trial discovery,
//! or result collection can be added without putting a branch in the runner.

use super::{Trial, prepare_external_source};
use std::path::Path;

mod agentbench;
mod agentdojo;
mod aider_polyglot;
mod bigcodebench;
mod core_bench;
mod evalplus;
mod livecodebench;
mod swe_bench;
mod swe_bench_lite;
mod swe_bench_pro;
mod swe_bench_verified;
mod swe_efficiency;
mod swe_lancer;
mod swe_perf;
mod swe_rebench;
mod tau_bench;
mod terminal_bench;

pub(super) trait WorkloadAdapter: Sync {
    fn id(&self) -> &'static str;
    fn trial_ids(&self) -> Vec<Trial>;
    fn prepare_source(&self, workloads: &Path, workspace: &Path) -> Result<usize, String>;
}

pub(super) struct StaticExternalAdapter {
    id: &'static str,
    trials: &'static [(&'static str, &'static str)],
}

impl StaticExternalAdapter {
    pub(super) const fn new(
        id: &'static str,
        trials: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { id, trials }
    }
}

impl WorkloadAdapter for StaticExternalAdapter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn trial_ids(&self) -> Vec<Trial> {
        self.trials
            .iter()
            .map(|(id, description)| Trial {
                id: (*id).into(),
                description: (*description).into(),
            })
            .collect()
    }
    fn prepare_source(&self, workloads: &Path, workspace: &Path) -> Result<usize, String> {
        prepare_external_source(self.id, workloads, workspace)
    }
}

static REGISTRY: &[&dyn WorkloadAdapter] = &[
    &agentbench::ADAPTER,
    &agentdojo::ADAPTER,
    &aider_polyglot::ADAPTER,
    &bigcodebench::ADAPTER,
    &core_bench::ADAPTER,
    &evalplus::ADAPTER,
    &livecodebench::ADAPTER,
    &swe_bench::ADAPTER,
    &swe_bench_lite::ADAPTER,
    &swe_bench_pro::ADAPTER,
    &swe_bench_verified::ADAPTER,
    &swe_efficiency::ADAPTER,
    &swe_lancer::ADAPTER,
    &swe_perf::ADAPTER,
    &swe_rebench::ADAPTER,
    &tau_bench::ADAPTER,
    &terminal_bench::ADAPTER,
];

pub(super) fn find(name: &str) -> Option<&'static dyn WorkloadAdapter> {
    REGISTRY
        .iter()
        .copied()
        .find(|adapter| adapter.id() == name)
}

#[cfg(test)]
pub(super) fn names() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|adapter| adapter.id())
}
