//! Single-source registry for every `asb tool install` target.

use crate::install::Recipe;
use crate::project;
use std::path::Path;

mod bpftrace;
mod cli2key;
mod flamegraph;
mod iostat;
mod mpstat;
mod perf;
mod perfetto;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ToolKind {
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

/// Resolve a tool executable through its registered project-local layout.
pub(crate) fn resolve(id: &str, project_root: &Path, tools_dir: &str) -> Option<String> {
    let tool = find(id)?;
    let executable = tool.executable()?;
    let root = project::resolve(project_root, tools_dir).join(tool.id());
    [
        root.join("bin").join(executable),
        root.join("node_modules/.bin").join(executable),
        root.join("venv/bin").join(executable),
        Path::new(".asb-tools/bin").join(executable),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
    .and_then(|candidate| candidate.canonicalize().ok())
    .map(|candidate| candidate.to_string_lossy().into_owned())
}
