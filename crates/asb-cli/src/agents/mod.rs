//! Extensible adapters for benchmark agents.
//!
//! The runner owns only process lifecycle and output capture. Every command
//! shape and provider-specific adjustment lives in the owning adapter module.

mod adapters;
use adapters::REGISTRY;

use crate::Error;
use crate::benchmark::BenchmarkConfig;
use crate::endpoint::{Interface, Support};
use crate::install::Recipe;
use crate::project;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) trait AgentAdapter: Sync {
    fn id(&self) -> &'static str;
    fn executable(&self) -> &'static str;
    /// Project-local installation recipe for this exact adapter executable.
    fn recipe(&self) -> Recipe;
    /// Supported wire interfaces, in preference order.
    fn supported_endpoints(&self) -> &'static [Support];
    fn command(&self, context: &CommandContext<'_>) -> CommandSpec;
    fn prepare(&self, context: &mut PreparationContext<'_>) -> Result<(), Error>;
    fn replay_credential_vars(&self) -> &'static [&'static str] {
        &[]
    }
    fn collect(&self, _context: &CollectionContext<'_>) -> AgentArtifacts {
        AgentArtifacts::default()
    }
}

pub(crate) struct PreparationContext<'a> {
    pub command: &'a mut Command,
    pub model: &'a str,
    pub connection: &'a crate::endpoint::Connection,
    /// Private mutable configuration for this agent execution.
    pub runtime_dir: &'a Path,
}

pub(crate) struct CommandContext<'a> {
    pub config: &'a BenchmarkConfig,
    pub endpoint: Option<&'a str>,
    pub interface: Option<Interface>,
    pub prompt: &'a str,
}

pub(crate) struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub append_prompt: bool,
}
#[derive(Default)]
pub(crate) struct AgentArtifacts {
    pub token_usage: Option<crate::measurements::TokenUsage>,
    /// Files that were consumed by this adapter and can be removed once their
    /// data has been incorporated into the generic run result.
    pub cleanup_paths: Vec<PathBuf>,
}

pub(crate) struct CollectionContext<'a> {
    pub workspace: &'a Path,
}

type CommandBuilder = for<'a> fn(&CommandContext<'a>) -> CommandSpec;
type PreparationHook = for<'a> fn(&mut PreparationContext<'a>) -> Result<(), Error>;
type CollectionHook = for<'a> fn(&CollectionContext<'a>) -> AgentArtifacts;

/// Trait-object implementation whose behavior is supplied by individual files.
pub(super) struct StaticAdapter {
    id: &'static str,
    executable: &'static str,
    recipe: Recipe,
    supported_endpoints: &'static [Support],
    command: CommandBuilder,
    prepare: PreparationHook,
    collect: Option<CollectionHook>,
    replay_credential_vars: &'static [&'static str],
}
impl StaticAdapter {
    pub(super) const fn new(
        id: &'static str,
        executable: &'static str,
        recipe: Recipe,
        supported_endpoints: &'static [Support],
        command: CommandBuilder,
        prepare: PreparationHook,
    ) -> Self {
        Self {
            id,
            executable,
            recipe,
            supported_endpoints,
            command,
            prepare,
            collect: None,
            replay_credential_vars: &[],
        }
    }

    pub(super) const fn with_collection(mut self, collect: CollectionHook) -> Self {
        self.collect = Some(collect);
        self
    }

    pub(super) const fn with_replay_credential_vars(
        mut self,
        vars: &'static [&'static str],
    ) -> Self {
        self.replay_credential_vars = vars;
        self
    }
}
impl AgentAdapter for StaticAdapter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn executable(&self) -> &'static str {
        self.executable
    }
    fn recipe(&self) -> Recipe {
        self.recipe
    }
    fn supported_endpoints(&self) -> &'static [Support] {
        self.supported_endpoints
    }
    fn command(&self, context: &CommandContext<'_>) -> CommandSpec {
        (self.command)(context)
    }
    fn prepare(&self, context: &mut PreparationContext<'_>) -> Result<(), Error> {
        (self.prepare)(context)
    }
    fn replay_credential_vars(&self) -> &'static [&'static str] {
        self.replay_credential_vars
    }
    fn collect(&self, context: &CollectionContext<'_>) -> AgentArtifacts {
        self.collect
            .map(|collect| collect(context))
            .unwrap_or_default()
    }
}
pub(crate) fn prepare(agent: &str, context: &mut PreparationContext<'_>) -> Result<(), Error> {
    let Some(adapter) = find(agent) else {
        return Ok(());
    };
    adapter.prepare(context)
}

/// Mask known agent-owned credential aliases in the inherited replay environment.
pub(crate) fn mask_replay_credentials(command: &mut Command) {
    for adapter in REGISTRY {
        for variable in adapter.replay_credential_vars() {
            command.env_remove(variable);
        }
    }
}

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
    let tools_root = project::resolve(project_root, tools_dir);
    candidates.extend(executable_candidates(id, executable, &tools_root));
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

fn executable_candidates(id: &str, executable: &str, tools_root: &Path) -> Vec<PathBuf> {
    let root = tools_root.join(id);
    let mut candidates = vec![
        root.join("bin").join(executable),
        root.join("node_modules/.bin").join(executable),
        root.join("venv/bin").join(executable),
        Path::new(".asb-tools/bin").join(executable),
    ];
    // The npm Codex launcher injects package-management metadata and can
    // inherit host integration state.  Prefer its bundled native executable
    // so benchmark runs are isolated from the enclosing Codex session.
    if id == "codex" {
        let (platform_package, target) = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => ("codex-linux-x64", "x86_64-unknown-linux-musl"),
            ("linux", "aarch64") => ("codex-linux-arm64", "aarch64-unknown-linux-musl"),
            ("macos", "x86_64") => ("codex-darwin-x64", "x86_64-apple-darwin"),
            ("macos", "aarch64") => ("codex-darwin-arm64", "aarch64-apple-darwin"),
            ("windows", "x86_64") => ("codex-win32-x64", "x86_64-pc-windows-msvc"),
            ("windows", "aarch64") => ("codex-win32-arm64", "aarch64-pc-windows-msvc"),
            _ => ("", ""),
        };
        if !platform_package.is_empty() {
            candidates.insert(
                0,
                root.join("node_modules")
                    .join("@openai")
                    .join(platform_package)
                    .join("vendor")
                    .join(target)
                    .join("bin")
                    .join(executable),
            );
        }
    }
    candidates
}
pub(crate) fn resolve(id: &str, project_root: &Path, tools_dir: &str) -> Option<String> {
    let adapter = find(id)?;
    resolve_executable(adapter.id(), adapter.executable(), project_root, tools_dir)
}

#[cfg(test)]
mod tests {
    #[test]
    fn replay_agent_configs_use_the_tape_endpoint_without_live_credentials() {
        let root =
            std::env::temp_dir().join(format!("asb-agent-replay-config-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for (agent, config) in [
            ("pi", "pi/config/models.json"),
            ("cline", "cline/config/data/settings/providers.json"),
            ("continue", "continue/config/config.yaml"),
        ] {
            let runtime = root.join(agent);
            let mut command = std::process::Command::new("unused");
            let adapter = super::find(agent).unwrap();
            let connection = crate::endpoint::negotiate(
                adapter.id(),
                adapter.supported_endpoints(),
                "openrouter",
                "test-model",
                None,
                Some((crate::tape::Mode::Replay, "http://127.0.0.1:4242/v1")),
            )
            .unwrap();
            super::prepare(
                agent,
                &mut super::PreparationContext {
                    command: &mut command,
                    model: "test-model",
                    connection: &connection,
                    runtime_dir: &runtime,
                },
            )
            .unwrap();
            let contents = std::fs::read_to_string(runtime.join(config)).unwrap();
            assert!(
                contents.contains("http://127.0.0.1:4242/v1"),
                "{agent}: {contents}"
            );
            assert!(
                !contents.contains("https://openrouter.ai"),
                "{agent}: {contents}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
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
                interface: Some(Interface::AnthropicMessages),
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

    #[test]
    fn every_agent_declares_and_prepares_every_advertised_interface() {
        let root =
            std::env::temp_dir().join(format!("asb-agent-interface-matrix-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for adapter in all() {
            assert!(
                !adapter.supported_endpoints().is_empty(),
                "{}",
                adapter.id()
            );
            for (index, interface) in adapter.supported_endpoints().iter().enumerate() {
                let runtime = root.join(adapter.id()).join(index.to_string());
                let connection = crate::endpoint::Connection {
                    interface: interface.interface,
                    base_url: "http://127.0.0.1:4242/v1".into(),
                    credential: Some("asb-replay".into()),
                    authentication: interface.authentication[0],
                };
                let mut command = std::process::Command::new("unused");
                adapter
                    .prepare(&mut PreparationContext {
                        command: &mut command,
                        model: "example/model",
                        connection: &connection,
                        runtime_dir: &runtime,
                    })
                    .unwrap_or_else(|error| panic!("{} {interface:?}: {error}", adapter.id()));
            }
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
