use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "pi",
    Recipe::Npm {
        package: "@mariozechner/pi-coding-agent@0.73.1",
        executable: "pi",
    },
    ToolKind::Agent,
);
