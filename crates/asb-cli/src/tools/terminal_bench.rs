use super::{StaticTool, ToolKind};
use crate::install::Recipe;

/// The original Terminal-Bench harness, not Harbor or a newer task suite.
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "terminal-bench",
    Recipe::Python {
        package: "terminal-bench==0.2.18",
        dependencies: &["docker==7.1.0"],
        executable: "tb",
    },
    ToolKind::Support,
);
