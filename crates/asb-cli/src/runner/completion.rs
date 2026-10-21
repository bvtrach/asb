//! Cleanup-confirmed task outcome records; distinct from input/cache seals.
use super::{RunResult, render_json, resolve_log_path};
use crate::Error;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Context<'a> {
    pub replica: super::replica::Identity,
    pub outputs: &'a [PathBuf],
}

pub(super) fn directory(config: &Path, log: &str) -> PathBuf {
    resolve_log_path(config, log)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".asb-completions")
}

pub(super) fn persist(
    config: &Path,
    log: &str,
    result: &RunResult<'_>,
    workload_cleanup_confirmed: bool,
    evidence: &[PathBuf],
    proxy: &crate::proxy::Evidence,
    context: &Context<'_>,
) -> Result<PathBuf, Error> {
    if !context.replica.valid() {
        return Err(Error::Input("invalid completion replica identity".into()));
    }
    let directory = directory(config, log);
    let log = resolve_log_path(config, log);
    if let Ok(metadata) = std::fs::symlink_metadata(&directory)
        && !metadata.is_dir()
    {
        return Err(Error::Input(
            "task completion directory must not be a symlink or non-directory".into(),
        ));
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory).map_err(crate::io_error)?;
    let mut summary = Vec::new();
    render_json(result, &mut summary)?;
    let summary: serde_json::Value = serde_json::from_slice(&summary)
        .map_err(|_| Error::Io("cannot encode task completion summary".into()))?;
    if let Some(recording) = &result.recording {
        recording.validate_summary(&summary, &proxy.captures)?;
    }
    if !crate::workloads::ResourceDisposition::matches_record(
        Some(&serde_json::json!(result.workload_resource_disposition)),
        workload_cleanup_confirmed,
        &summary,
    ) {
        return Err(Error::Input(
            "task settlement disposition contradicts cleanup confirmation".into(),
        ));
    }
    let proxy_outputs = proxy
        .output_files
        .iter()
        .filter(|path| {
            result
                .recording
                .as_ref()
                .is_none_or(|recording| **path != recording.producer)
        })
        .map(std::path::absolute)
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::io_error)?;
    let mut outputs = vec![std::path::absolute(&log).map_err(crate::io_error)?];
    outputs.extend(
        context
            .outputs
            .iter()
            .map(std::path::absolute)
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::io_error)?,
    );
    outputs.sort();
    outputs.dedup();
    let content = serde_json::to_vec_pretty(&json!({"schema_version":2,
        "replica":context.replica,"outputs":outputs,
        "scope":"task-outcome-not-yet-a-resume-receipt",
        "workload_cleanup_confirmed":workload_cleanup_confirmed,
        "workload_resource_disposition":result.workload_resource_disposition,
        "proxy_cleanup_confirmed":proxy.cleanup_confirmed,
        "proxy_evidence":proxy_outputs,
        "capture_evidence":proxy.captures,
        "recording_track":result.recording,
        "summary":summary,"workload_evidence":evidence}))
    .map_err(|_| Error::Io("cannot encode private task completion evidence".into()))?;
    let id = crate::workloads::TaskIdentity::local("completion-file", None).attempt_id;
    let temporary = directory.join(format!(".{id}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(crate::io_error)?;
    let destination = directory.join(format!("{id}.json"));
    file.write_all(&content)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&temporary, &destination))
        .and_then(|()| std::fs::File::open(&directory)?.sync_all())
        .map_err(crate::io_error)?;
    Ok(destination)
}
