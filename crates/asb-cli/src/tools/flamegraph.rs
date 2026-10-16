use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool =
    StaticTool::new("flamegraph", Recipe::Flamegraph, ToolKind::Collector);
