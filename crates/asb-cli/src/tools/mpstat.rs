use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "mpstat",
    Recipe::Host {
        executable: "mpstat",
    },
    ToolKind::Collector,
);
