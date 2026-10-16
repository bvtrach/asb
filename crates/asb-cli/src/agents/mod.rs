//! Extensible adapters for benchmark agents.
//!
//! The runner owns only process lifecycle and output capture. Every command
//! shape and provider-specific adjustment lives in the owning adapter module.

mod claude_code;
mod cline;
mod codex;
mod continue_agent;
mod goose;
mod mini_swe;
mod opencode;
mod openhands;
mod openjiuwen;
mod pi;
mod qwen_code;
mod workbuddy;

use crate::Error;
use crate::benchmark::BenchmarkConfig;
use crate::project;
use std::path::Path;
use std::process::Command;

pub(crate) trait AgentAdapter: Sync {
    fn id(&self) -> &'static str;
    fn executable(&self) -> &'static str;
    fn command(&self, context: &CommandContext<'_>) -> CommandSpec;
    fn prepare(&self, context: &mut PreparationContext<'_>) -> Result<(), Error>;
    fn collect(&self, _output: &str) -> AgentArtifacts {
        AgentArtifacts::default()
    }
}

pub(crate) struct PreparationContext<'a> {
    pub command: &'a mut Command,
    pub provider: &'a str,
    pub model: &'a str,
    pub cli2key: Option<(&'a str, &'a str)>,
    pub pi_config_dir: Option<&'a Path>,
    pub workbuddy_config_dir: Option<&'a Path>,
    pub continue_config_dir: Option<&'a Path>,
    pub cline_data_dir: Option<&'a Path>,
}

pub(crate) struct CommandContext<'a> {
    pub config: &'a BenchmarkConfig,
    pub endpoint: Option<&'a str>,
    pub prompt: &'a str,
}

pub(crate) struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub append_prompt: bool,
}
#[derive(Default)]
pub(crate) struct AgentArtifacts {
    pub paths: Vec<String>,
}

type CommandBuilder = for<'a> fn(&CommandContext<'a>) -> CommandSpec;
type PreparationHook = for<'a> fn(&mut PreparationContext<'a>) -> Result<(), Error>;

/// Trait-object implementation whose behavior is supplied by individual files.
pub(super) struct StaticAdapter {
    id: &'static str,
    executable: &'static str,
    command: CommandBuilder,
    prepare: PreparationHook,
}
impl StaticAdapter {
    pub(super) const fn new(
        id: &'static str,
        executable: &'static str,
        command: CommandBuilder,
        prepare: PreparationHook,
    ) -> Self {
        Self {
            id,
            executable,
            command,
            prepare,
        }
    }
}
impl AgentAdapter for StaticAdapter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn executable(&self) -> &'static str {
        self.executable
    }
    fn command(&self, context: &CommandContext<'_>) -> CommandSpec {
        (self.command)(context)
    }
    fn prepare(&self, context: &mut PreparationContext<'_>) -> Result<(), Error> {
        (self.prepare)(context)
    }
}
pub(super) fn no_prepare(_context: &mut PreparationContext<'_>) -> Result<(), Error> {
    Ok(())
}

pub(crate) fn prepare(
    command: &mut Command,
    agent: &str,
    provider: &str,
    model: &str,
    cli2key: Option<(&str, &str)>,
    pi_config_dir: Option<&Path>,
    workbuddy_config_dir: Option<&Path>,
    continue_config_dir: Option<&Path>,
    cline_data_dir: Option<&Path>,
) -> Result<(), Error> {
    let Some(adapter) = find(agent) else {
        return Ok(());
    };
    adapter.prepare(&mut PreparationContext {
        command,
        provider,
        model,
        cli2key,
        pi_config_dir,
        workbuddy_config_dir,
        continue_config_dir,
        cline_data_dir,
    })
}

static REGISTRY: &[&dyn AgentAdapter] = &[
    &codex::ADAPTER,
    &pi::ADAPTER,
    &opencode::ADAPTER,
    &workbuddy::ADAPTER,
    &qwen_code::ADAPTER,
    &mini_swe::ADAPTER,
    &openhands::ADAPTER,
    &openjiuwen::ADAPTER,
    &claude_code::ADAPTER,
    &goose::ADAPTER,
    &cline::ADAPTER,
    &continue_agent::ADAPTER,
];
pub(crate) fn all() -> &'static [&'static dyn AgentAdapter] {
    REGISTRY
}
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|adapter| adapter.id())
}
pub(crate) fn find(id: &str) -> Option<&'static dyn AgentAdapter> {
    REGISTRY.iter().copied().find(|adapter| adapter.id() == id)
}

pub(crate) fn resolve_executable(
    id: &str,
    executable: &str,
    project_root: &Path,
    tools_dir: &str,
) -> Option<String> {
    let mut candidates = Vec::new();
    if let Some(directory) = std::env::var_os("ASB_TOOL_DIR") {
        candidates.push(std::path::PathBuf::from(directory).join(executable));
    }
    let root = project::resolve(project_root, tools_dir).join(id);
    candidates.extend([
        root.join("bin").join(executable),
        root.join("node_modules/.bin").join(executable),
        root.join("venv/bin").join(executable),
        Path::new(".asb-tools/bin").join(executable),
    ]);
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}
pub(crate) fn resolve(id: &str, project_root: &Path, tools_dir: &str) -> Option<String> {
    let adapter = find(id)?;
    resolve_executable(adapter.id(), adapter.executable(), project_root, tools_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_adapter_builds_a_noninteractive_command() {
        let config = BenchmarkConfig {
            agent: "unused".into(),
            workload: "bug-fix".into(),
            provider: "cli2key".into(),
            model: "test-model".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "run.log".into(),
            measurements: None,
        };
        for adapter in all() {
            let command = adapter.command(&CommandContext {
                config: &config,
                endpoint: Some("http://127.0.0.1:3000/v1"),
                prompt: "repair",
            });
            assert!(!command.program.is_empty(), "{}", adapter.id());
            assert!(!command.args.is_empty(), "{}", adapter.id());
            assert!(!adapter.executable().is_empty(), "{}", adapter.id());
        }
    }

    #[test]
    fn registry_has_unique_stable_ids() {
        let ids = names().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), all().len());
        assert!(ids.contains("codex"));
        assert!(ids.contains("continue"));
    }
}
