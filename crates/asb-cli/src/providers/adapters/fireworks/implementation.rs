//! Fireworks account-scoped model discovery.
pub(in crate::providers) fn catalog_base(selected: Option<&str>) -> Option<String> {
    let account = std::env::var("FIREWORKS_ACCOUNT_ID")
        .ok()
        .or_else(|| selected.and_then(model_account).map(str::to_owned))
        .unwrap_or_else(|| "fireworks".into());
    if account.is_empty()
        || !account
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    Some(format!(
        "https://api.fireworks.ai/v1/accounts/{account}/models"
    ))
}

pub(in crate::providers) fn model_account(model: &str) -> Option<&str> {
    let mut parts = model.split('/');
    (parts.next()? == "accounts").then_some(())?;
    let account = parts.next()?;
    (parts.next()? == "models"
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_none())
    .then_some(account)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fireworks_account_is_derived_only_from_complete_model_resources() {
        assert_eq!(
            super::model_account("accounts/my-account/models/model"),
            Some("my-account")
        );
        assert_eq!(super::model_account("accounts/my-account/models/"), None);
        assert_eq!(
            super::model_account("accounts/my-account/models/model/extra"),
            None
        );
        assert_eq!(super::model_account("unqualified-model"), None);
    }
}
