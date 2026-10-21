use super::{StaticTool, ToolKind};
use crate::Error;
use crate::install::{LoggedStatus, Recipe, install_source, make_executable, verify_sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PERFETTO_LINUX_X86_64_URL: &str =
    "https://commondatastorage.googleapis.com/perfetto-luci-artifacts/v58.2/linux-amd64/tracebox";
const PERFETTO_LINUX_X86_64_SHA256: &str =
    "5348e5f3513dec2e70f02d3e4296cfed546e6ebc7f9a7cc6b5499f363b4aea03";
const PERFETTO_LINUX_AARCH64_URL: &str =
    "https://commondatastorage.googleapis.com/perfetto-luci-artifacts/v58.2/linux-arm64/tracebox";
const PERFETTO_LINUX_AARCH64_SHA256: &str =
    "2d21883e9c55818f851fd8c391afb9b366032b3110076b4fd41f31f30b5849fa";
const PERFETTO_PROCESSOR_LINUX_X86_64_URL: &str = "https://commondatastorage.googleapis.com/perfetto-luci-artifacts/v58.2/linux-amd64/trace_processor_shell";
const PERFETTO_PROCESSOR_LINUX_X86_64_SHA256: &str =
    "58042408e6cc861fb1a731c26bb082dc222285561eaa4e12a48a8b2b90dca7b9";
const PERFETTO_PROCESSOR_LINUX_AARCH64_URL: &str = "https://commondatastorage.googleapis.com/perfetto-luci-artifacts/v58.2/linux-arm64/trace_processor_shell";
const PERFETTO_PROCESSOR_LINUX_AARCH64_SHA256: &str =
    "0e6e0c5452c505c8d46fe472fd196a0d17d963460727e2ce2013b02aa1309555";

pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "perfetto",
    Recipe::Custom {
        executable: "tracebox",
        description: "download pinned Perfetto Tracebox and trace processor and verify their SHA-256 checksums",
        summary: "pinned Perfetto Tracebox + trace processor",
        install: install_perfetto,
    },
    ToolKind::Collector,
).with_helper_executables(&["trace_processor_shell"]);

/// Download a pinned official Perfetto Tracebox binary into the benchmark
/// project. Tracebox is the Linux Perfetto collector used by the optional
/// scheduler and userspace tracepoint measurement adapter.
fn install_perfetto(root: &Path, progress: &mut dyn FnMut(u8, &str)) -> Result<PathBuf, Error> {
    let ((tracebox_url, tracebox_hash), (processor_url, processor_hash)) =
        match std::env::consts::ARCH {
            "x86_64" => (
                (PERFETTO_LINUX_X86_64_URL, PERFETTO_LINUX_X86_64_SHA256),
                (
                    PERFETTO_PROCESSOR_LINUX_X86_64_URL,
                    PERFETTO_PROCESSOR_LINUX_X86_64_SHA256,
                ),
            ),
            "aarch64" => (
                (PERFETTO_LINUX_AARCH64_URL, PERFETTO_LINUX_AARCH64_SHA256),
                (
                    PERFETTO_PROCESSOR_LINUX_AARCH64_URL,
                    PERFETTO_PROCESSOR_LINUX_AARCH64_SHA256,
                ),
            ),
            architecture => {
                return Err(Error::Input(format!(
                    "Perfetto Tracebox is currently packaged by ASB only for Linux x86_64 and aarch64; this project is running on {architecture}"
                )));
            }
        };
    let bin = root.join("bin");
    fs::create_dir_all(&bin)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", bin.display())))?;
    let destination = bin.join("tracebox");
    progress(20, "downloading pinned Perfetto Tracebox");
    install_source(tracebox_url, &destination, "Perfetto Tracebox")?;
    progress(45, "verifying Perfetto Tracebox checksum");
    verify_sha256(&destination, tracebox_hash, "Perfetto Tracebox")?;
    make_executable(&destination)?;
    let processor = bin.join("trace_processor_shell");
    progress(60, "downloading pinned Perfetto trace processor");
    install_source(processor_url, &processor, "Perfetto trace processor")?;
    progress(80, "verifying Perfetto trace processor checksum");
    verify_sha256(&processor, processor_hash, "Perfetto trace processor")?;
    make_executable(&processor)?;
    progress(90, "checking project-local Perfetto tools");
    let version = Command::new(&destination)
        .arg("--version")
        .logged_status()
        .map_err(|error| Error::Io(format!("cannot start installed Perfetto Tracebox: {error}")))?;
    if !version.success() {
        return Err(Error::Io(
            "installed Perfetto Tracebox did not report a usable version".into(),
        ));
    }
    let processor_version = Command::new(&processor)
        .arg("--version")
        .logged_status()
        .map_err(|error| {
            Error::Io(format!(
                "cannot start installed Perfetto trace processor: {error}"
            ))
        })?;
    if !processor_version.success() {
        return Err(Error::Io(
            "installed Perfetto trace processor did not report a usable version".into(),
        ));
    }
    Ok(destination)
}

#[cfg(test)]
#[path = "../../tests/unit/perfetto_install.rs"]
mod coverage_tests;
