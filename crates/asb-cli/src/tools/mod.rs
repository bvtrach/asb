//! Single-source registry for every `asb tool install` target.

use crate::install::Recipe;
use crate::project;
use std::path::Path;

pub(crate) mod bpftrace;
mod cli2key;
mod flamegraph;
mod harbor;
mod iostat;
mod mpstat;
mod perf;
mod perfetto;
mod swebench;
mod terminal_bench;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ToolKind {
    Collector,
    Support,
}

pub(crate) trait ToolAdapter: Sync {
    fn id(&self) -> &'static str;
    fn recipe(&self) -> Recipe;
    fn kind(&self) -> ToolKind;
    fn post_install(
        &self,
        _executable: &Path,
        _progress: &mut dyn FnMut(u8, &str),
        _output: &mut dyn std::io::Write,
    ) -> Result<(), crate::Error> {
        Ok(())
    }

    /// Executable published by this tool, when it has one. This keeps the
    /// installer, availability check, and runner on the same catalog entry.
    fn executable(&self) -> Option<&'static str> {
        match self.recipe() {
            Recipe::Npm { executable, .. }
            | Recipe::Python { executable, .. }
            | Recipe::Host { executable }
            | Recipe::Custom { executable, .. } => Some(executable),
            Recipe::Goose => Some("goose"),
        }
    }
}

type PostInstallHook =
    fn(&Path, &mut dyn FnMut(u8, &str), &mut dyn std::io::Write) -> Result<(), crate::Error>;

pub(crate) struct StaticTool {
    id: &'static str,
    recipe: Recipe,
    kind: ToolKind,
    post_install: Option<PostInstallHook>,
}
impl StaticTool {
    pub(crate) const fn new(id: &'static str, recipe: Recipe, kind: ToolKind) -> Self {
        Self {
            id,
            recipe,
            kind,
            post_install: None,
        }
    }
    pub(crate) const fn with_post_install(mut self, hook: PostInstallHook) -> Self {
        self.post_install = Some(hook);
        self
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
    fn post_install(
        &self,
        executable: &Path,
        progress: &mut dyn FnMut(u8, &str),
        output: &mut dyn std::io::Write,
    ) -> Result<(), crate::Error> {
        self.post_install
            .map_or(Ok(()), |hook| hook(executable, progress, output))
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
    &swebench::ADAPTER,
    &terminal_bench::ADAPTER,
    &harbor::ADAPTER,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_benchmark_harnesses_have_pinned_project_local_python_recipes() {
        for (id, expected_binary, expected_package, expected_dependencies) in [
            (
                "swebench",
                "swebench",
                "swebench @ git+https://github.com/SWE-bench/SWE-bench.git@02e7a74ffd0b707aab73d203fe87bdc7c76afc8e",
                &["docker==7.1.0"][..],
            ),
            ("terminal-bench", "tb", "terminal-bench==0.2.18", &[][..]),
            ("harbor", "harbor", "harbor==0.24.0", &[][..]),
        ] {
            let adapter = find(id).unwrap();
            assert!(adapter.kind() == ToolKind::Support);
            assert_eq!(adapter.executable(), Some(expected_binary));
            match adapter.recipe() {
                Recipe::Python {
                    package,
                    dependencies,
                    executable,
                } => {
                    assert_eq!(package, expected_package);
                    assert_eq!(executable, expected_binary);
                    assert_eq!(dependencies, expected_dependencies);
                }
                _ => panic!("{id} must install the actual official Python harness"),
            }
        }
    }
}
