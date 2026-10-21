//! Shared identifier rule: `all` is a selector, never a selectable identifier.
pub(crate) const ALL: &str = "all";

pub(crate) fn validate(name: &str, kind: &str) -> Result<(), String> {
    if name == ALL {
        return Err(format!(
            "{kind} name `all` is reserved for selecting every entry; rename the entry"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn registered_selection_names_never_use_the_reserved_keyword() {
        let names = crate::agents::all()
            .iter()
            .map(|adapter| adapter.id())
            .chain(crate::providers::names().iter().copied())
            .chain(crate::install::external_workload_names())
            .chain(crate::workloads::builtin_names())
            .chain(crate::tools::names());
        for name in names {
            super::validate(name, "registered selection").unwrap();
        }
        for name in crate::measurements::available_collectors() {
            super::validate(&name, "collector").unwrap();
        }
        for workload in crate::install::external_workload_names() {
            for trial in crate::workloads::trial_ids(workload) {
                super::validate(&trial.id, "default trial").unwrap();
            }
        }
        for provider in crate::providers::names() {
            super::validate(crate::providers::default_model(provider), "default model").unwrap();
            for model in crate::providers::known_models(provider)
                .into_iter()
                .flatten()
            {
                super::validate(model, "model snapshot").unwrap();
            }
        }
    }

    #[test]
    fn reserved_name_is_rejected_but_substrings_are_valid() {
        assert!(
            super::validate("all", "model")
                .unwrap_err()
                .contains("reserved")
        );
        for name in ["small", "all-tools", "org/all"] {
            super::validate(name, "model").unwrap();
        }
    }
}
