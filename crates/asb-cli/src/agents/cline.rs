use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("cline", "cline", CommandKind::Cline);
