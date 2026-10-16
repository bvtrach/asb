use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "cline",
    Recipe::Npm {
        package: "cline@3.0.72",
        executable: "cline",
    },
    ToolKind::Agent,
);
