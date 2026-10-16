use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "claude-code",
    Recipe::Npm {
        package: "@anthropic-ai/claude-code@2.1.301",
        executable: "claude",
    },
    ToolKind::Agent,
);
