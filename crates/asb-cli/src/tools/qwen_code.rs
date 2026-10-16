use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "qwen-code",
    Recipe::Npm {
        package: "@qwen-code/qwen-code@0.25.1",
        executable: "qwen",
    },
    ToolKind::Agent,
);
