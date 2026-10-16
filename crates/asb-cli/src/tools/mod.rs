//! Single-source registry for every `asb tool install` target.

use crate::install::Recipe;

mod bpftrace;
mod claude_code;
mod cli2key;
mod cline;
mod codex;
mod continue_agent;
mod flamegraph;
mod goose;
mod iostat;
mod mini_swe;
mod mpstat;
mod opencode;
mod openhands;
mod openjiuwen;
mod perf;
mod perfetto;
mod pi;
mod qwen_code;
mod workbuddy;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ToolKind {
    Agent,
    Collector,
    Support,
}

pub(crate) trait ToolAdapter: Sync {
    fn id(&self) -> &'static str;
    fn recipe(&self) -> Recipe;
    fn kind(&self) -> ToolKind;

    /// Executable published by this tool, when it has one. This keeps the
    /// installer, availability check, and runner on the same catalog entry.
    fn executable(&self) -> Option<&'static str> {
        match self.recipe() {
            Recipe::Npm { executable, .. }
            | Recipe::Python { executable, .. }
            | Recipe::Host { executable } => Some(executable),
            Recipe::Bpftrace => Some("bpftrace"),
            Recipe::Perfetto => Some("tracebox"),
            Recipe::Flamegraph => Some("flamegraph"),
            Recipe::Goose => Some("goose"),
        }
    }
}

pub(crate) struct StaticTool {
    id: &'static str,
    recipe: Recipe,
    kind: ToolKind,
}
impl StaticTool {
    pub(crate) const fn new(id: &'static str, recipe: Recipe, kind: ToolKind) -> Self {
        Self { id, recipe, kind }
    }
}
impl ToolAdapter for StaticTool {
    fn id(&self) -> &'static str {
        self.id
    }
    fn recipe(&self) -> Recipe {
        self.recipe
    }
    fn kind(&self) -> ToolKind {
        self.kind
    }
}

static REGISTRY: &[&dyn ToolAdapter] = &[
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
    &cli2key::ADAPTER,
    &mpstat::ADAPTER,
    &iostat::ADAPTER,
    &perf::ADAPTER,
    &bpftrace::ADAPTER,
    &perfetto::ADAPTER,
    &flamegraph::ADAPTER,
];
pub(crate) fn all() -> &'static [&'static dyn ToolAdapter] {
    REGISTRY
}
pub(crate) fn find(id: &str) -> Option<&'static dyn ToolAdapter> {
    REGISTRY.iter().copied().find(|tool| tool.id() == id)
}
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|tool| tool.id())
}
