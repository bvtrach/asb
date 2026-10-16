use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "codex",
    Recipe::Npm {
        package: "@openai/codex@0.162.1",
        executable: "codex",
    },
    ToolKind::Agent,
);
