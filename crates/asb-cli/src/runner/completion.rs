//! Cleanup-confirmed task outcome records; distinct from input/cache seals.
use super::{RunResult, render_json, resolve_log_path};
use crate::Error;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn persist(
    config: &Path,
    log: &str,
    result: &RunResult<'_>,
    workload_cleanup_confirmed: bool,
    evidence: &[PathBuf],
) -> Result<(), Error> {
    let log = resolve_log_path(config, log);
    let directory = log
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".asb-completions");
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
    let content = serde_json::to_vec_pretty(&json!({"schema_version":1,
        "scope":"task-outcome-not-yet-a-resume-receipt",
        "workload_cleanup_confirmed":workload_cleanup_confirmed,
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
    file.write_all(&content)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&temporary, directory.join(format!("{id}.json"))))
        .map_err(crate::io_error)
}
