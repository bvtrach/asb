//! Immutable source catalogs for the two original Terminal-Bench versions.
use super::TerminalBenchAdapter;
use crate::install::{self, LoggedStatus};
use crate::workloads::session::DatasetIdentity;
use crate::workloads::{InstallationContext, TaskIdentity, Trial, TrialSet};
use crate::{Error, project};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;

const HELPER: &str = include_str!("../../../../assets/benchmarks/asb_terminal_catalog.py");
const ORIGINAL_TASKS: &str =
    include_str!("../../../../assets/benchmarks/terminal_bench_1_tasks.json");
const LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Catalog {
    schema_version: u32,
    workload: String,
    pub dataset: DatasetIdentity,
    sdk: Sdk,
    source_sha256: String,
    archive_sha256: String,
    pub tasks: Vec<Task>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sdk {
    package: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Task {
    pub id: String,
    pub description: String,
    instruction: String,
    definition: String,
}

pub(super) fn read(adapter: &TerminalBenchAdapter, directory: &Path) -> Result<Catalog, String> {
    let root = directory.join(adapter.id);
    let path = root.join("catalog.json");
    let failure = || {
        format!(
            "{} has no valid official task catalog; run `asb workload install {}`",
            adapter.id, adapter.id
        )
    };
    if fs::symlink_metadata(&path)
        .map_err(|_| failure())?
        .file_type()
        .is_symlink()
    {
        return Err(failure());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| failure())?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure())?;
    if bytes.len() as u64 > LIMIT {
        return Err(failure());
    }
    let catalog: Catalog = serde_json::from_slice(&bytes).map_err(|_| failure())?;
    let hash =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if catalog.schema_version != 1
        || catalog.workload != adapter.id
        || catalog.dataset.source != adapter.source
        || catalog.dataset.revision != adapter.revision
        || catalog.dataset.split != adapter.split
        || catalog.sdk.package != adapter.tool
        || catalog.sdk.version != adapter.version
        || catalog.tasks.len() != adapter.expected_tasks
        || !hash(&catalog.source_sha256)
        || !hash(&catalog.archive_sha256)
    {
        return Err(failure());
    }
    let canonical = root.canonicalize().map_err(|_| failure())?;
    let manifest =
        fs::symlink_metadata(root.join("source-manifest.json")).map_err(|_| failure())?;
    if !manifest.is_file() || manifest.len() > LIMIT {
        return Err(failure());
    }
    let mut ids = BTreeSet::new();
    for task in &catalog.tasks {
        if task.id.is_empty()
            || task.id.len() > 256
            || !task
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
            || !ids.insert(&task.id)
            || task.description.len() > 1024
            || task.instruction.trim().is_empty()
            || task.instruction.len() > 64 * 1024
            || task.definition != format!("source/tasks/{}", task.id)
            || !root.join(&task.definition).is_dir()
            || !root
                .join(&task.definition)
                .canonicalize()
                .is_ok_and(|path| path.starts_with(&canonical))
        {
            return Err(failure());
        }
    }
    Ok(catalog)
}

fn publish(path: &Path, content: &str) -> Result<(), Error> {
    crate::workloads::controller::publish_asset(path, content)
}

pub(super) fn ensure_helpers(root: &Path) -> Result<(), Error> {
    crate::workloads::controller::publish_support(root)?;
    publish(&root.join("asb_terminal_catalog.py"), HELPER)?;
    publish(&root.join("terminal_bench_1_tasks.json"), ORIGINAL_TASKS)?;
    publish(
        &root.join("asb_terminal_bench_1.py"),
        include_str!("../../../../assets/benchmarks/asb_terminal_bench_1.py"),
    )
}

pub(super) fn install(
    adapter: &TerminalBenchAdapter,
    context: &InstallationContext<'_>,
    output: &mut dyn Write,
    progress: &mut dyn FnMut(u8, &str),
) -> Result<(), Error> {
    let _network = project::network_project(context.project_root);
    if !install::tool_installed_locally(adapter.tool, context.tools_root) {
        install::tool_with_progress(
            install::ToolInstallOptions {
                name: adapter.tool.into(),
                source: None,
                dir: Some(context.tools_root.to_path_buf()),
                dry_run: false,
            },
            output,
            progress,
        )?;
    }
    let tool = context.tools_root.join(adapter.tool);
    let helper = tool.join("asb_terminal_catalog.py");
    ensure_helpers(&tool)?;
    let directory = context.workloads_root.join(adapter.id);
    let run = |operation: &str, archive: Option<&Path>| -> Result<bool, Error> {
        let mut command = Command::new(tool.join("venv/bin/python"));
        command
            .arg(&helper)
            .arg(operation)
            .arg("--workload")
            .arg(adapter.id)
            .arg("--directory")
            .arg(&directory)
            .env("PYTHONDONTWRITEBYTECODE", "1");
        if let Some(archive) = archive {
            command.arg("--archive").arg(archive);
        }
        Ok(command
            .logged_status()
            .map_err(|error| Error::Io(format!("cannot run official catalog validator: {error}")))?
            .success())
    };
    if read(adapter, context.workloads_root).is_err() || !run("verify", None)? {
        let cache = context.project_root.join(".asb-cache/workload-sources");
        fs::create_dir_all(&cache).map_err(|error| Error::Io(error.to_string()))?;
        let archive = cache.join(format!("{}-{}.tar.gz", adapter.id, adapter.revision));
        if archive.exists() && !run("archive", Some(&archive))? {
            let backup = archive.with_extension(format!(
                "damaged-{}",
                TaskIdentity::local(adapter.id, None).attempt_id
            ));
            fs::rename(&archive, &backup).map_err(|error| Error::Io(error.to_string()))?;
            writeln!(
                output,
                "Preserved damaged source archive at {} before downloading it again.",
                backup.display()
            )
            .map_err(crate::io_error)?;
        }
        if !archive.is_file() {
            progress(20, "downloading pinned official task source");
            let temporary = archive.with_extension(format!(
                "part-{}",
                TaskIdentity::local(adapter.id, None).attempt_id
            ));
            let status = Command::new("curl")
                .args([
                    "--fail",
                    "--location",
                    "--silent",
                    "--show-error",
                    "--max-filesize",
                    "268435456",
                    "--output",
                ])
                .arg(&temporary)
                .arg(adapter.archive)
                .logged_status()
                .map_err(|error| {
                    Error::Io(format!("cannot download official task source: {error}"))
                })?;
            if !status.success() {
                return Err(Error::Io(format!(
                    "{} source download failed; check network/proxy access and .asb/install.log",
                    adapter.id
                )));
            }
            if !run("archive", Some(&temporary))? {
                return Err(Error::Io("downloaded task archive is incomplete or unsafe; retained evidence in the project cache; retry the installation".into()));
            }
            fs::rename(&temporary, &archive).map_err(|error| Error::Io(error.to_string()))?;
        }
        progress(50, "validating original task definitions with official SDK");
        if !run("install", Some(&archive))? {
            return Err(Error::Input(format!(
                "{} dataset validation failed; inspect .asb/install.log; reinstall `{}` with `asb tool install {}` if its SDK is incompatible",
                adapter.id, adapter.tool, adapter.tool
            )));
        }
    }
    let catalog = read(adapter, context.workloads_root).map_err(Error::Input)?;
    fs::write(
        context.workloads_root.join(format!("{}.txt", adapter.id)),
        format!(
            "Official {} task catalog. Select an installed instance.\n",
            adapter.id
        ),
    )
    .map_err(|error| Error::Io(error.to_string()))?;
    selection(adapter.id, &catalog.tasks, context.workloads_root, output)?;
    progress(100, "official task catalog installed");
    writeln!(
        output,
        "Installed {}: {} official tasks.",
        adapter.id,
        catalog.tasks.len()
    )
    .map_err(crate::io_error)
}

fn selection(
    id: &str,
    tasks: &[Task],
    directory: &Path,
    output: &mut dyn Write,
) -> Result<(), Error> {
    let path = directory.join(format!("{id}.workload.toml"));
    if path.exists() {
        let metadata = fs::symlink_metadata(&path).map_err(|error| Error::Io(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > LIMIT {
            return Err(Error::Input(
                "existing task selection must be a bounded regular file".into(),
            ));
        }
        let previous = fs::read_to_string(&path).map_err(|error| Error::Io(error.to_string()))?;
        let available = tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<BTreeSet<_>>();
        if toml::from_str::<TrialSet>(&previous).is_ok_and(|selected| {
            selected.workload == id
                && !selected.trials.is_empty()
                && selected
                    .trials
                    .iter()
                    .all(|trial| available.contains(trial.id.as_str()))
                && selected
                    .trials
                    .iter()
                    .map(|trial| &trial.id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == selected.trials.len()
        }) {
            return Ok(());
        }
        let backup = path.with_extension(format!(
            "toml.backup-{}",
            TaskIdentity::local(id, None).attempt_id
        ));
        fs::rename(&path, &backup).map_err(|error| Error::Io(error.to_string()))?;
        writeln!(
            output,
            "Preserved the previous incompatible task selection at {}.",
            backup.display()
        )
        .map_err(crate::io_error)?;
    }
    let selected = TrialSet {
        workload: id.into(),
        trials: tasks
            .iter()
            .map(|task| Trial {
                id: task.id.clone(),
                description: task.description.clone(),
            })
            .collect(),
    };
    publish(
        &path,
        &toml::to_string_pretty(&selected).map_err(|error| Error::Io(error.to_string()))?,
    )
}
