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
mod node;
mod perf;
mod perfetto;
mod runtime;
mod swebench;
mod terminal_bench;
mod uv;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ToolKind {
    Collector,
    Support,
}

pub(crate) trait ToolAdapter: Sync {
    fn id(&self) -> &'static str;
    fn recipe(&self) -> Recipe;
    fn kind(&self) -> ToolKind;
    fn runtime_path(&self, _root: &Path) -> Option<std::path::PathBuf> {
        None
    }
    fn host_runtime_usable(&self) -> bool {
        self.executable().is_some_and(host_program_on_path)
    }
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
    runtime_path: bool,
    host_probe: Option<fn() -> bool>,
}
impl StaticTool {
    pub(crate) const fn new(id: &'static str, recipe: Recipe, kind: ToolKind) -> Self {
        Self {
            id,
            recipe,
            kind,
            post_install: None,
            runtime_path: false,
            host_probe: None,
        }
    }
    pub(crate) const fn with_post_install(mut self, hook: PostInstallHook) -> Self {
        self.post_install = Some(hook);
        self
    }
    pub(crate) const fn with_runtime_path(mut self) -> Self {
        self.runtime_path = true;
        self
    }
    pub(crate) const fn with_host_probe(mut self, probe: fn() -> bool) -> Self {
        self.host_probe = Some(probe);
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
    fn runtime_path(&self, root: &Path) -> Option<std::path::PathBuf> {
        self.runtime_path
            .then(|| root.join("bin"))
            .filter(|path| path.is_dir())
    }
    fn host_runtime_usable(&self) -> bool {
        if let Some(probe) = self.host_probe {
            return probe();
        }
        self.executable().is_some_and(host_program_on_path)
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

fn host_program_on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| {
            let Ok(metadata) = std::fs::metadata(directory.join(program)) else {
                return false;
            };
            if !metadata.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
    })
}

static REGISTRY: &[&dyn ToolAdapter] = &[
    &node::ADAPTER,
    &uv::ADAPTER,
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

pub(crate) fn apply_runtime_path(
    command: &mut std::process::Command,
    root: &Path,
    tools_dir: &str,
) -> Result<(), crate::Error> {
    let tools = project::resolve(root, tools_dir);
    let mut paths = REGISTRY
        .iter()
        .filter_map(|tool| tool.runtime_path(&tools.join(tool.id())))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Ok(());
    }
    let original = command
        .get_envs()
        .find(|(name, _)| *name == "PATH")
        .map(|(_, value)| value.map(std::ffi::OsStr::to_os_string))
        .unwrap_or_else(|| std::env::var_os("PATH"));
    if let Some(original) = original {
        paths.extend(std::env::split_paths(&original));
    }
    command.env(
        "PATH",
        std::env::join_paths(paths)
            .map_err(|error| crate::Error::Input(format!("invalid runtime PATH: {error}")))?,
    );
    Ok(())
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
            (
                "terminal-bench",
                "tb",
                "terminal-bench==0.2.18",
                &["docker==7.1.0"][..],
            ),
            ("harbor", "harbor", "harbor==0.24.0", &["docker==7.1.0"][..]),
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
