//! Adapters for externally sourced benchmark suites.
//!
//! Each supported suite has a module so suite-specific setup, trial discovery,
//! or result collection can be added without putting a branch in the runner.

use super::{
    Trial, populate_builtin_fixtures, populate_builtin_prompts, prepare_builtin_fixture,
    prepare_external_source,
};
use std::path::Path;

mod agentbench;
mod agentdojo;
mod aider_polyglot;
mod bigcodebench;
mod core_bench;
mod evalplus;
mod livecodebench;
mod prompt;
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
    /// Smoke fixtures are not instances from the named upstream benchmark.
    fn evaluation_kind(&self) -> &'static str {
        "adapter-smoke"
    }
    fn matches(&self, name: &str) -> bool {
        self.id() == name
    }
    fn trial_ids(&self) -> Vec<Trial>;
    fn populate_prompts(&self, _directory: &Path) -> Result<usize, std::io::Error> {
        Ok(0)
    }
    fn populate_fixtures(&self, _directory: &Path) -> Result<usize, std::io::Error> {
        Ok(0)
    }
    fn prepare_fixture(&self, _name: &str, _workspace: &Path) -> Result<usize, String> {
        Ok(0)
    }
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

pub(super) struct PromptAdapter;
impl WorkloadAdapter for PromptAdapter {
    fn evaluation_kind(&self) -> &'static str {
        "bundled-fixture"
    }
    fn id(&self) -> &'static str {
        "prompt"
    }
    fn matches(&self, name: &str) -> bool {
        super::is_builtin_prompt(name)
    }
    fn trial_ids(&self) -> Vec<Trial> {
        default_trials()
    }
    fn populate_prompts(&self, directory: &Path) -> Result<usize, std::io::Error> {
        populate_builtin_prompts(directory)
    }
    fn populate_fixtures(&self, directory: &Path) -> Result<usize, std::io::Error> {
        populate_builtin_fixtures(directory)
    }
    fn prepare_fixture(&self, name: &str, workspace: &Path) -> Result<usize, String> {
        prepare_builtin_fixture(name, workspace)
    }
    fn prepare_source(&self, _workloads: &Path, _workspace: &Path) -> Result<usize, String> {
        Ok(0)
    }
}

fn default_trials() -> Vec<Trial> {
    [
        ("smoke", "Small local smoke trial."),
        ("standard", "Standard workload trial."),
        ("extended", "Extended workload trial."),
    ]
    .into_iter()
    .map(|(id, description)| Trial {
        id: id.into(),
        description: description.into(),
    })
    .collect()
}

static REGISTRY: &[&dyn WorkloadAdapter] = &[
    &prompt::ADAPTER,
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
        .find(|adapter| adapter.matches(name))
}

pub(super) fn populate_prompts(directory: &Path) -> Result<usize, std::io::Error> {
    REGISTRY.iter().try_fold(0, |count, adapter| {
        Ok(count + adapter.populate_prompts(directory)?)
    })
}

pub(super) fn populate_fixtures(directory: &Path) -> Result<usize, std::io::Error> {
    REGISTRY.iter().try_fold(0, |count, adapter| {
        Ok(count + adapter.populate_fixtures(directory)?)
    })
}

#[cfg(test)]
pub(super) fn names() -> impl Iterator<Item = &'static str> {
    REGISTRY
        .iter()
        .map(|adapter| adapter.id())
        .filter(|id| *id != "prompt")
}
