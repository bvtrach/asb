use super::{StaticTool, ToolKind};
use crate::Error;
use crate::install::{Recipe, install_source, make_executable};
use std::path::{Path, PathBuf};

pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "flamegraph",
    Recipe::Custom {
        executable: "flamegraph",
        description: "download pinned FlameGraph scripts",
        summary: "pinned FlameGraph scripts -> flamegraph",
        install,
    },
    ToolKind::Collector,
)
.with_helper_executables(&["stackcollapse-perf.pl"]);

fn install(root: &Path, progress: &mut dyn FnMut(u8, &str)) -> Result<PathBuf, Error> {
    progress(25, "downloading FlameGraph scripts");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin)
        .map_err(|error| Error::Io(format!("cannot create {}: {error}", bin.display())))?;
    for (name, url) in [
        (
            "stackcollapse-perf.pl",
            "https://raw.githubusercontent.com/brendangregg/FlameGraph/41fee1f99f9276008b7cd112fca19dc3ea84ac32/stackcollapse-perf.pl",
        ),
        (
            "flamegraph",
            "https://raw.githubusercontent.com/brendangregg/FlameGraph/41fee1f99f9276008b7cd112fca19dc3ea84ac32/flamegraph.pl",
        ),
    ] {
        let destination = bin.join(name);
        install_source(url, &destination, "flamegraph tool")?;
        make_executable(&destination)?;
        progress(
            if name == "flamegraph" { 90 } else { 60 },
            "installing FlameGraph scripts",
        );
    }
    Ok(bin.join("flamegraph"))
}
