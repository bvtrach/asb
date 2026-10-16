use super::{StaticTool, ToolKind};
use crate::install::Recipe;
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "openhands",
    Recipe::Python {
        package: "openhands==1.16.0",
        executable: "openhands",
    },
    ToolKind::Agent,
);
