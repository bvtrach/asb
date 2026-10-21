//! Validated, project-local installer budgets, with backwards-compatible defaults.
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Execution budgets in seconds, reloaded between installation operations.
///
/// Cleanup grace is independent. Raising both operation and build budgets permits
/// long source builds; these values are not hidden fixed execution caps.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct InstallationConfig {
    /// Shared budget for the complete installation operation.
    pub operation_timeout_seconds: u64,
    /// General inspection budget.
    pub probe_timeout_seconds: u64,
    /// Staged executable version inspection budget.
    pub version_timeout_seconds: u64,
    /// Primary-source download budget.
    pub download_timeout_seconds: u64,
    /// Archive extraction budget.
    pub extract_timeout_seconds: u64,
    /// Source configuration budget.
    pub configure_timeout_seconds: u64,
    /// Dependency/package installation budget.
    pub package_timeout_seconds: u64,
    /// Source compilation budget.
    pub build_timeout_seconds: u64,
}

impl Default for InstallationConfig {
    fn default() -> Self {
        Self {
            operation_timeout_seconds: 10800,
            probe_timeout_seconds: 30,
            version_timeout_seconds: 10,
            download_timeout_seconds: 300,
            extract_timeout_seconds: 300,
            configure_timeout_seconds: 900,
            package_timeout_seconds: 2700,
            build_timeout_seconds: 7200,
        }
    }
}

impl InstallationConfig {
    /// Validate all fields without truncating or silently clamping them.
    pub fn validate(&self) -> Result<(), String> {
        let now = Instant::now();
        for (key, seconds) in self.values() {
            if seconds == 0 || now.checked_add(Duration::from_secs(seconds)).is_none() {
                return Err(format!(
                    "installation.{key} must be a nonzero, representable duration in seconds"
                ));
            }
        }
        Ok(())
    }

    fn values(&self) -> [(&'static str, u64); 8] {
        [
            ("operation_timeout_seconds", self.operation_timeout_seconds),
            ("probe_timeout_seconds", self.probe_timeout_seconds),
            ("version_timeout_seconds", self.version_timeout_seconds),
            ("download_timeout_seconds", self.download_timeout_seconds),
            ("extract_timeout_seconds", self.extract_timeout_seconds),
            ("configure_timeout_seconds", self.configure_timeout_seconds),
            ("package_timeout_seconds", self.package_timeout_seconds),
            ("build_timeout_seconds", self.build_timeout_seconds),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_and_partial_sections_have_documented_defaults() {
        let empty: InstallationConfig = toml::from_str("").unwrap();
        assert_eq!(empty, InstallationConfig::default());
        let partial: InstallationConfig = toml::from_str("build_timeout_seconds=14400").unwrap();
        assert_eq!(partial.build_timeout_seconds, 14400);
        assert_eq!(partial.operation_timeout_seconds, 10800);
        partial.validate().unwrap();
    }

    #[test]
    fn rejects_zero_overflow_and_wrong_toml_types_without_clamping() {
        for (key, _) in InstallationConfig::default().values() {
            for value in ["0", "-1", "1.5", "\"30\""] {
                let parsed = toml::from_str::<InstallationConfig>(&format!("{key}={value}"));
                assert!(
                    match parsed {
                        Err(_) => true,
                        Ok(config) => config.validate().is_err(),
                    },
                    "{key}={value}"
                );
            }
        }
        let invalid = InstallationConfig {
            build_timeout_seconds: u64::MAX,
            ..InstallationConfig::default()
        };
        assert!(
            invalid
                .validate()
                .unwrap_err()
                .contains("build_timeout_seconds")
        );
    }

    #[test]
    fn project_load_reloads_and_reports_the_precise_invalid_config_key() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/installer-budget-config")
            .join(crate::tools::runtime::unique_id());
        std::fs::create_dir_all(root.join(".asb")).unwrap();
        let path = root.join(".asb/project.toml");
        let mut config = crate::project::ProjectConfig::default();
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let (_, first) = crate::project::load(&root).unwrap();
        config.installation.build_timeout_seconds = 14400;
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let (_, second) = crate::project::load(&root).unwrap();
        assert_eq!(first.installation.build_timeout_seconds, 7200);
        assert_eq!(second.installation.build_timeout_seconds, 14400);
        config.installation.build_timeout_seconds = 0;
        std::fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let error = crate::project::load(&root).unwrap_err().to_string();
        assert!(error.contains(path.to_str().unwrap()));
        assert!(error.contains("installation.build_timeout_seconds"));
        // Evidence is intentionally retained in the owned target namespace.
    }
}
