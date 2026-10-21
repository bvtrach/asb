use super::{StaticTool, ToolKind};
use crate::install::Recipe;

/// The official task/environment/verifier framework for Terminal-Bench 2.1.
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "harbor",
    Recipe::Python {
        package: "harbor==0.24.0",
        dependencies: &["docker==7.1.0"],
        executable: "harbor",
    },
    ToolKind::Support,
);
