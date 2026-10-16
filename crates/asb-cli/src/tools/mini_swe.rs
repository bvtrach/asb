use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "mini-swe",
    Recipe::Python {
        package: "mini-swe-agent==2.4.6",
        executable: "mini",
    },
    ToolKind::Agent,
);
