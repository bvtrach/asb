use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("continue", "cn", CommandKind::Continue);
