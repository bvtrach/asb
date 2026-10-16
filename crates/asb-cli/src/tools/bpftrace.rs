use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool =
    StaticTool::new("bpftrace", Recipe::Bpftrace, ToolKind::Collector);
