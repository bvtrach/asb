use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new("goose", Recipe::Goose, ToolKind::Agent);
