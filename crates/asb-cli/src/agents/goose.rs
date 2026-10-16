use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("goose", "goose", CommandKind::Goose);
