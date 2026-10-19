use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "iostat",
    Recipe::Host {
        executable: "iostat",
    },
    ToolKind::Collector,
)
.with_post_install(super::mpstat::post_install);
