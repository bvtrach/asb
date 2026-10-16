use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "openjiuwen",
    Recipe::Python {
        package: "openjiuwen[cli]==0.1.19.post2",
        executable: "openjiuwen",
    },
    ToolKind::Agent,
);
