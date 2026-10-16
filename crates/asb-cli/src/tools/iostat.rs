use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "iostat",
    Recipe::Host {
        executable: "iostat",
    },
    ToolKind::Collector,
);
