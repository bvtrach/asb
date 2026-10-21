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
.with_host_probe(host_usable)
.with_guest_assets(guest_assets);

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

fn distribution() -> Result<(&'static str, &'static str), Error> {
    Ok(match std::env::consts::ARCH {
        "x86_64" => (
            "https://nodejs.org/dist/v22.20.0/node-v22.20.0-linux-x64.tar.gz",
            "eeaccb0378b79406f2208e8b37a62479c70595e20be6b659125eb77dd1ab2a29",
        ),
        "aarch64" => (
            "https://nodejs.org/dist/v22.20.0/node-v22.20.0-linux-arm64.tar.gz",
            "4181609e03dcb9880e7e5bf956061ecc0503c77a480c6631d868cb1f65a2c7dd",
        ),
        _ => {
            return Err(Error::Input(
                "Node.js runtime requires Linux x86_64 or aarch64".into(),
            ));
        }
    })
}

fn guest_assets(root: &Path) -> Result<Vec<super::GuestAsset>, Error> {
    let (url, checksum) = distribution()?;
    Ok(super::runtime::guest_archive(root, url, checksum))
}

fn install(
    root: &Path,
    progress: &mut dyn FnMut(u8, crate::install::ProgressStage),
) -> Result<PathBuf, Error> {
    let (url, hash) = distribution()?;
    super::runtime::archive(
        root,
        url,
        hash,
        &[("node", "bin/node"), ("npm", "bin/npm"), ("npx", "bin/npx")],
        progress,
    )
}
