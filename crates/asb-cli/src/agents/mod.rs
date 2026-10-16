//! Extensible adapters for benchmark agents.
//!
//! Generic runner code owns process lifecycle, output capture, measurements,
//! and provider wiring. Each adapter owns its stable identity, executable and
//! non-interactive command shape; adding an agent is therefore a new module
//! and registry entry instead of another runner branch.

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

use crate::benchmark::BenchmarkConfig;
use crate::project;
use std::path::Path;

pub(crate) trait AgentAdapter: Sync {
    fn id(&self) -> &'static str;
    fn executable(&self) -> &'static str;
    fn command(&self, context: &CommandContext<'_>) -> CommandSpec;

    /// Hook for adapter-owned result or artifact extraction. The generic runner
    /// calls this after output is captured; built-ins currently need no extra
    /// artifact extraction but custom adapters can opt in without runner edits.
    fn collect(&self, _output: &str) -> AgentArtifacts {
        AgentArtifacts::default()
    }
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

#[derive(Clone, Copy)]
pub(super) enum CommandKind {
    Codex,
    Pi,
    Opencode,
    Workbuddy,
    QwenCode,
    MiniSwe,
    Openhands,
    Openjiuwen,
    ClaudeCode,
    Goose,
    Cline,
    Continue,
}

pub(super) struct BuiltinAdapter {
    id: &'static str,
    executable: &'static str,
    kind: CommandKind,
}

impl BuiltinAdapter {
    pub(super) const fn new(id: &'static str, executable: &'static str, kind: CommandKind) -> Self {
        Self {
            id,
            executable,
            kind,
        }
    }
}

impl AgentAdapter for BuiltinAdapter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn executable(&self) -> &'static str {
        self.executable
    }

    fn command(&self, context: &CommandContext<'_>) -> CommandSpec {
        let config = context.config;
        match self.kind {
            CommandKind::Codex => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "exec".into(),
                    "--skip-git-repo-check".into(),
                    "--json".into(),
                    "--model".into(),
                    config.model.clone(),
                ],
                append_prompt: true,
            },
            CommandKind::Pi => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--provider".into(),
                    config.provider.clone(),
                    "--model".into(),
                    config.model.clone(),
                    "--mode".into(),
                    "json".into(),
                    "--print".into(),
                ],
                append_prompt: true,
            },
            CommandKind::Opencode => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "run".into(),
                    "--model".into(),
                    opencode_model(config),
                    "--format".into(),
                    "json".into(),
                ],
                append_prompt: true,
            },
            CommandKind::Workbuddy => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--print".into(),
                    "--output-format".into(),
                    "json".into(),
                    "--model".into(),
                    config.model.clone(),
                    "-y".into(),
                ],
                append_prompt: true,
            },
            CommandKind::QwenCode => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--model".into(),
                    config.model.clone(),
                    "--output-format".into(),
                    "json".into(),
                    "--yolo".into(),
                ],
                append_prompt: true,
            },
            CommandKind::MiniSwe => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--model".into(),
                    format!("anthropic/{}", config.model),
                    "--task".into(),
                    context.prompt.into(),
                    "--yolo".into(),
                    "--exit-immediately".into(),
                    "--output".into(),
                    ".asb-mini-swe.traj.json".into(),
                ],
                append_prompt: false,
            },
            CommandKind::Openhands => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--headless".into(),
                    "--json".into(),
                    "--yolo".into(),
                    "--override-with-envs".into(),
                    "--task".into(),
                    context.prompt.into(),
                ],
                append_prompt: false,
            },
            CommandKind::Openjiuwen => CommandSpec {
                program: self.executable.into(),
                args: [
                    "--model",
                    config.model.as_str(),
                    "--provider",
                    "anthropic",
                    "--api-base",
                    context.endpoint.unwrap_or_default(),
                    "run",
                    "-f",
                    "json",
                    context.prompt,
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                append_prompt: false,
            },
            CommandKind::ClaudeCode => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "-p".into(),
                    "--output-format".into(),
                    "json".into(),
                    "--model".into(),
                    config.model.clone(),
                    "--dangerously-skip-permissions".into(),
                    context.prompt.into(),
                ],
                append_prompt: false,
            },
            CommandKind::Goose => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "run".into(),
                    "--no-session".into(),
                    "--output-format".into(),
                    "json".into(),
                    "-t".into(),
                    context.prompt.into(),
                ],
                append_prompt: false,
            },
            CommandKind::Cline => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "--json".into(),
                    "--auto-approve".into(),
                    "true".into(),
                    "--provider".into(),
                    "openai".into(),
                    "--model".into(),
                    config.model.clone(),
                    context.prompt.into(),
                ],
                append_prompt: false,
            },
            CommandKind::Continue => CommandSpec {
                program: self.executable.into(),
                args: vec![
                    "-p".into(),
                    context.prompt.into(),
                    "--model".into(),
                    config.model.clone(),
                    "--format".into(),
                    "json".into(),
                    "--auto".into(),
                ],
                append_prompt: false,
            },
        }
    }
}

fn opencode_model(config: &BenchmarkConfig) -> String {
    if config.model.contains('/') {
        return config.model.clone();
    }
    if config.provider.eq_ignore_ascii_case("cli2key") {
        format!("cli2key/{}", config.model)
    } else if config.provider.eq_ignore_ascii_case("opencode-zen") {
        format!("opencode/{}", config.model)
    } else {
        config.model.clone()
    }
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
