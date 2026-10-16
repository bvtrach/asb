use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("codex", "codex", CommandKind::Codex);
