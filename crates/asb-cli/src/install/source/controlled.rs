//! One frozen installation operation and the SAME actual source-stage owner.
use super::*;
use crate::install::process::{
    InstallFailure, InstallResult, OperationContext, ProcessScope, Purpose,
};
use std::sync::Arc;

pub(crate) fn check_version_in(
    scope: &mut ProcessScope,
    path: &Path,
    label: &str,
) -> InstallResult<()> {
    let mut step = scope.prepare(path, Purpose::Version, label)?;
    step.command_mut().arg("--version").stdin(Stdio::null());
    let result = scope.run(step)?;
    if !result.status.success() {
        return Err(Error::Io(format!("staged {label} did not report a usable version ({}); previous installation was preserved; inspect {}", result.status,
            result.log.path.as_ref().map_or_else(|| "the installation diagnostics".into(), |path| path.display().to_string()))).into());
    }
    Ok(())
}

fn download_in(
    scope: &mut ProcessScope,
    source: &str,
    staging: &Path,
    options: &Options<'_>,
) -> InstallResult<()> {
    let mut step = scope.prepare("curl", Purpose::Download, options.kind)?;
    step.command_mut()
        .args([
            "--disable",
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-filesize",
        ])
        .arg(options.limit.to_string())
        .arg("--output")
        .arg(staging)
        .arg(source)
        .stdin(Stdio::null());
    let result = scope.run(step)?;
    if !result.status.success() {
        return Err(Error::Io(format!("could not download {}: curl exited with {}; previous installation was preserved; inspect {}", options.kind, result.status,
            result.log.path.as_ref().map_or_else(|| "the installation diagnostics".into(), |path| path.display().to_string()))).into());
    }
    Ok(())
}

pub(in crate::install) fn install_input_in(
    context: &OperationContext,
    source: &Input,
    destination: &Path,
    options: Options<'_>,
    validate: impl FnOnce(&Path, &mut ProcessScope) -> InstallResult<()>,
    publication: Publication,
) -> InstallResult<()> {
    let staging = Arc::new(Staging::new(destination)?);
    staging.monitor_producer(options.kind, options.limit)?;
    let mut scope = staging.producer_scope(context);
    let prepared = (|| -> InstallResult<prepared::Snapshot> {
        match source {
            Input::Remote(url) => download_in(&mut scope, url, &staging.file, &options)?,
            Input::Local(path) | Input::Pinned { path, .. } => {
                let mut input_options = OpenOptions::new();
                input_options.read(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    input_options.custom_flags(libc::O_NONBLOCK);
                }
                let mut input = if let Input::Pinned { handle, .. } = source {
                    use std::io::{Seek, SeekFrom};
                    let mut file = handle.try_clone().map_err(crate::io_error)?;
                    file.seek(SeekFrom::Start(0)).map_err(crate::io_error)?;
                    file
                } else {
                    input_options.open(path).map_err(|error| {
                        Error::Input(format!(
                            "cannot read {} source {}: {error}",
                            options.kind,
                            path.display()
                        ))
                    })?
                };
                let metadata = input.metadata().map_err(crate::io_error)?;
                if !metadata.is_file() {
                    return Err(Error::Input(format!(
                        "{} source is not a file: {}",
                        options.kind,
                        path.display()
                    ))
                    .into());
                }
                if metadata.len() > options.limit {
                    return Err(size_error(options.kind, options.limit).into());
                }
                let mut output = staging.writer().map_err(crate::io_error)?;
                let copied = io::copy(
                    &mut (&mut input).take(options.limit.saturating_add(1)),
                    &mut output,
                )
                .map_err(|error| {
                    Error::Io(format!("cannot copy {} source: {error}", options.kind))
                })?;
                checkpoint(FaultPoint::Copy, &staging.file)?;
                if copied > options.limit {
                    return Err(size_error(options.kind, options.limit).into());
                }
                if copied != metadata.len()
                    || !prepared::unchanged_metadata(
                        &metadata,
                        &input.metadata().map_err(crate::io_error)?,
                    )
                {
                    return Err(Error::Input(
                        "local installation source changed during bounded copying".into(),
                    )
                    .into());
                }
                output
                    .set_permissions(metadata.permissions())
                    .map_err(crate::io_error)?;
            }
        }
        staging.file_owned().map_err(crate::io_error)?;
        validate_file(&staging.file, &options)?;
        if options.executable {
            checkpoint(FaultPoint::Mode, &staging.file)?;
            staging.make_executable()?;
        }
        let pinned = staging.snapshot(&options)?;
        validate(&staging.file, &mut scope)?;
        let validated = staging.snapshot(&options)?;
        if !pinned.matches(&validated) {
            return Err(Error::Input("staged installation source changed after its version probe; previous installation was preserved".into()).into());
        }
        Ok(pinned)
    })();
    // Never attempt quiet cleanup or publication after typed uncertainty. The
    // retained worker owns this SAME real Staging; its eventual Drop keeps raw.
    if let Err(InstallFailure::Abort(abort)) = prepared {
        return Err(InstallFailure::Abort(abort));
    }
    let permission = scope.finish()?;
    staging.accept_producer_permission(permission)?;
    let mut staging = Arc::try_unwrap(staging).map_err(|stage| {
        context.custody_failure(
            "settled installation producer still retains its staging owner",
            &stage.directory,
        )
    })?;
    let result = match prepared {
        Ok(pinned) => (|| -> Result<(), Error> {
            staging.synchronize()?;
            checkpoint(FaultPoint::Commit, &staging.file)?;
            if !pinned.matches(&staging.snapshot(&options)?) {
                return Err(Error::Input("staged installation source changed before commit; previous installation was preserved".into()));
            }
            staging.file_owned().map_err(crate::io_error)?;
            staging.publish_copy(destination, &options, &pinned, publication)
        })(),
        Err(InstallFailure::Ordinary(error)) => Err(error),
        Err(InstallFailure::Abort(_)) => unreachable!("typed abort returned before settlement"),
    };
    staging.finish(result).map_err(Into::into)
}
