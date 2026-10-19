use super::super::{ArtifactDisplay, StaticCollector};
pub(super) static ADAPTER: StaticCollector =
    StaticCollector::new("flamegraph", None, Some("flamegraph"), true)
        .with_dependencies(&["perf"])
        .with_preflight(preflight)
        .with_artifacts(
            |name| name == "flamegraph.svg" || name == "flamegraph.json",
            |name| match name {
                "flamegraph.svg" => Some(ArtifactDisplay::Svg {
                    class: "flamegraph",
                    label: "Flamegraph SVG",
                }),
                "flamegraph.json" => Some(ArtifactDisplay::Status {
                    class: "flamegraph-status",
                    label: "Flamegraph collector status",
                    suffix: "An SVG is embedded here when collection completed.",
                }),
                _ => None,
            },
        )
        .with_finish(|session| session.write_flamegraph());

use crate::measurements::{self, write_json};
use serde::Serialize;
use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FLAMEGRAPH_TIMEOUT: Duration = Duration::from_secs(60);

fn flamegraph_commands(
    tool_dir: Option<&Path>,
) -> (Option<std::ffi::OsString>, Option<std::ffi::OsString>) {
    let collapse = std::env::var_os("ASB_FLAMEGRAPH_STACKCOLLAPSE").or_else(|| {
        tool_dir.and_then(|dir| {
            [
                dir.join("flamegraph/bin/stackcollapse-perf.pl"),
                dir.join("bin/stackcollapse-perf.pl"),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| path.into_os_string())
        })
    });
    let render = std::env::var_os("ASB_FLAMEGRAPH_RENDER").or_else(|| {
        tool_dir.and_then(|dir| {
            [
                dir.join("flamegraph/bin/flamegraph"),
                dir.join("bin/flamegraph"),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| path.into_os_string())
        })
    });
    (collapse, render)
}

fn preflight(context: &mut super::super::PreflightContext<'_>) -> Result<(), String> {
    if !context.selected.iter().any(|item| item == "flamegraph") {
        return Ok(());
    }
    let (Some(collapse), Some(render)) = flamegraph_commands(context.tool_dir) else {
        return Err("flamegraph scripts are required for the selected collector. Install them with `asb tool install flamegraph` and retry.".into());
    };
    for executable in [collapse, render] {
        let mut child = Command::new(&executable).arg("--help").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
            .map_err(|error| format!("cannot execute flamegraph tool {}: {error}. Check directory access and reinstall with `asb tool install flamegraph`.", Path::new(&executable).display()))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break, // Some script versions return nonzero for --help.
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "flamegraph tool {} did not finish its preflight; check `asb tool install flamegraph`.",
                        Path::new(&executable).display()
                    ));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("cannot check flamegraph tool: {error}"));
                }
            }
        }
    }
    Ok(())
}

impl measurements::Session {
    pub(crate) fn write_flamegraph(&self) -> Result<(), String> {
        let Some(data) = self.artifact(super::super::SharedArtifact::StackSamples) else {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "perf recording did not start; check perf.json for the host error",
            );
        };
        if !data.is_file() || data.metadata().map_or(true, |metadata| metadata.len() == 0) {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "perf produced no samples; check perf.json and host perf permissions",
            );
        }
        let (collapse, render) = flamegraph_commands(self.tool_dir.as_deref());
        let (Some(collapse), Some(render)) = (collapse, render) else {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "install stackcollapse-perf.pl and flamegraph in the project tool directory",
            );
        };
        let mut script = self
            .command("perf")
            .args(["script", "-i"])
            .arg(data)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start perf script for flamegraph: {e}"))?;
        let script_stdout = script
            .stdout
            .take()
            .ok_or_else(|| "perf script stdout was not piped".to_owned())?;
        let folded_path = self.dir.join("flamegraph.folded");
        let mut collapsed = Command::new(collapse)
            .stdin(Stdio::from(script_stdout))
            .stdout(Stdio::from(File::create(&folded_path).map_err(|e| {
                format!("cannot create flamegraph folded output: {e}")
            })?))
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start flamegraph stack collapse script: {e}"))?;
        if !wait_bounded(&mut script, FLAMEGRAPH_TIMEOUT)? {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "perf script exceeded the 60 second flamegraph deadline or failed",
            );
        }
        if !wait_bounded(&mut collapsed, FLAMEGRAPH_TIMEOUT)? {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "the stack collapse script exceeded the 60 second flamegraph deadline or failed",
            );
        }
        let mut render = Command::new(render);
        render
            .stdin(Stdio::from(File::open(&folded_path).map_err(|e| {
                format!("cannot open collapsed flamegraph output: {e}")
            })?))
            .stdout(Stdio::from(
                File::create(self.dir.join("flamegraph.svg"))
                    .map_err(|e| format!("cannot create flamegraph.svg: {e}"))?,
            ));
        let mut render = render
            .spawn()
            .map_err(|e| format!("cannot start flamegraph renderer: {e}"))?;
        if !wait_bounded(&mut render, FLAMEGRAPH_TIMEOUT)? {
            return write_flamegraph_status(
                &self.dir,
                "unavailable",
                None,
                "the flamegraph renderer exceeded the 60 second deadline or failed",
            );
        }
        write_flamegraph_status(&self.dir, "completed", Some("flamegraph.svg"), "")
    }
}

#[derive(Debug, Serialize)]
struct FlamegraphStatus<'a> {
    schema_version: u32,
    status: &'a str,
    artifact: Option<&'a str>,
    reason: Option<&'a str>,
}

pub(crate) fn wait_bounded(child: &mut Child, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("cannot poll collector process: {error}"))?
        {
            Some(status) => return Ok(status.success()),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let pid = child.id().to_string();
                let _ = Command::new("pkill").args(["-KILL", "-P", &pid]).status();
                let _ = child.wait();
                return Ok(false);
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

pub(crate) fn write_flamegraph_status(
    dir: &Path,
    status: &'static str,
    artifact: Option<&'static str>,
    reason: &'static str,
) -> Result<(), String> {
    write_json(
        &dir.join("flamegraph.json"),
        &FlamegraphStatus {
            schema_version: 1,
            status,
            artifact,
            reason: (!reason.is_empty()).then_some(reason),
        },
    )
}
