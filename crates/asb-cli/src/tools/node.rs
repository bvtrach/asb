//! Official Node.js distribution, including its matching npm/npx installation.
use super::{StaticTool, ToolKind};
use crate::{Error, install::Recipe};
use std::path::{Path, PathBuf};

pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "node",
    Recipe::Custom {
        executable: "node",
        description: "install the pinned official Node.js distribution and npm",
        summary: "Node.js 22.20.0 + npm",
        install,
    },
    ToolKind::Support,
)
.with_runtime_path()
.with_host_probe(host_usable);

fn host_usable() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .is_some_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .trim_start_matches('v')
                    .split('.')
                    .next()
                    .and_then(|major| major.parse::<u32>().ok())
                    .is_some_and(|major| major >= 22)
        })
}

fn install(root: &Path, progress: &mut dyn FnMut(u8, &str)) -> Result<PathBuf, Error> {
    let (url, hash) = match std::env::consts::ARCH {
        "x86_64" => (
            "https://nodejs.org/dist/v22.20.0/node-v22.20.0-linux-x64.tar.xz",
            "00bbd05e306ea68b6e13e17360d0e2f680b493ef95f2fea1c4296ff7437530bc",
        ),
        "aarch64" => (
            "https://nodejs.org/dist/v22.20.0/node-v22.20.0-linux-arm64.tar.xz",
            "06907b9c088ce62305bc1530e5c1ae1510245114645768f7750c349c5b6fe667",
        ),
        _ => {
            return Err(Error::Input(
                "Node.js runtime requires Linux x86_64 or aarch64".into(),
            ));
        }
    };
    super::runtime::archive(
        root,
        url,
        hash,
        &[("node", "bin/node"), ("npm", "bin/npm"), ("npx", "bin/npx")],
        progress,
    )
}
