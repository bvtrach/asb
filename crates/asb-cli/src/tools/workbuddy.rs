use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "workbuddy",
    Recipe::Npm {
        package: "@tencent-ai/codebuddy-code@2.167.0",
        executable: "codebuddy",
    },
    ToolKind::Agent,
);
