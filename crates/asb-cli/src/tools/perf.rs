use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "perf",
    Recipe::Host { executable: "perf" },
    ToolKind::Collector,
);
