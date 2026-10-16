use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter = BuiltinAdapter::new("pi", "pi", CommandKind::Pi);
