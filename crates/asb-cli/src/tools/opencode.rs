use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "opencode",
    Recipe::Npm {
        package: "opencode-ai@1.19.0",
        executable: "opencode",
    },
    ToolKind::Agent,
);
