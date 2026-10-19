use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "mpstat",
    Recipe::Host {
        executable: "mpstat",
    },
    ToolKind::Collector,
)
.with_post_install(post_install);

fn post_install(
    executable: &std::path::Path,
    _progress: &mut dyn FnMut(u8, &str),
    _output: &mut dyn std::io::Write,
) -> Result<(), crate::Error> {
    let source = crate::install::host_executable("pidstat").map_err(|_| {
        crate::Error::Io("mpstat installation also needs the matching sysstat `pidstat` binary for process-scoped collection; install sysstat and rerun `asb tool install mpstat`".into())
    })?;
    let destination = executable
        .parent()
        .ok_or_else(|| crate::Error::Io("mpstat executable has no parent directory".into()))?
        .join("pidstat");
    std::fs::copy(&source, &destination).map_err(|error| {
        crate::Error::Io(format!(
            "cannot copy process-scoped pidstat tool {} to {}: {error}",
            source.display(),
            destination.display()
        ))
    })?;
    crate::install::make_executable(&destination)
}
