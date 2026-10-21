use super::{StaticTool, ToolKind};
use crate::install::Recipe;

/// One pinned official harness serves Full, Lite and Verified datasets.
pub(super) static ADAPTER: StaticTool = StaticTool::new(
    "swebench",
    Recipe::Python {
        package: "swebench @ git+https://github.com/SWE-bench/SWE-bench.git@02e7a74ffd0b707aab73d203fe87bdc7c76afc8e",
        dependencies: &["docker==7.1.0"],
        executable: "swebench",
    },
    ToolKind::Support,
);
