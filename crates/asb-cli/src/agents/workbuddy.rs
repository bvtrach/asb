use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("workbuddy", "codebuddy", CommandKind::Workbuddy);
