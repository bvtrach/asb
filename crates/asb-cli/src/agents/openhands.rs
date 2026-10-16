use super::{BuiltinAdapter, CommandKind};
pub(super) static ADAPTER: BuiltinAdapter =
    BuiltinAdapter::new("openhands", "openhands", CommandKind::Openhands);
