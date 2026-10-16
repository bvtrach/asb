use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "cli2key",
    Recipe::Npm {
        package: "codex-anthropic-bridge@0.1.10",
        executable: "codex-bridge",
    },
    ToolKind::Support,
);
