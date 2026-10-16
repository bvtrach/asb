use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "continue",
    Recipe::Npm {
        package: "@continuedev/cli@1.5.47",
        executable: "cn",
    },
    ToolKind::Agent,
);
