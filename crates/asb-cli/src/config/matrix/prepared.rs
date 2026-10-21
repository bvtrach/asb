//! Pure selection serialization: no directory or destination mutation here.
use super::*;

pub(super) struct Sweep {
    pub(super) directory: PathBuf,
    pub(super) configurations: Vec<(String, String)>,
    pub(super) manifest: String,
}

impl Sweep {
    pub(super) fn new(path: &Path, mut configs: Vec<BenchmarkConfig>) -> Result<Self, Error> {
        if configs.is_empty() || configs.len() > crate::sweep::MAX_CONFIGS {
            return Err(Error::Input(
                "invalid setup configuration population".into(),
            ));
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let group = format!("{stem}-configs-{}", stamp());
        let directory = std::path::absolute(parent.join(&group)).map_err(crate::io_error)?;
        let mut configurations = Vec::with_capacity(configs.len());
        let mut references = Vec::with_capacity(configs.len());
        let mut total = 0usize;
        for (index, config) in configs.iter_mut().enumerate() {
            crate::planning::resolve(config)?;
            let filename = format!("benchmark-{index:03}.toml");
            config.log_file = format!("benchmark-{index:03}.log");
            if let Some(measures) = config.measurements.as_mut() {
                measures.directory = directory
                    .join(format!("measurements-{index:03}"))
                    .to_string_lossy()
                    .into_owned();
            }
            let text = render(config)?;
            total = total.checked_add(text.len()).ok_or_else(|| {
                Error::Input(
                    "setup configuration size overflow; no setup files were written".into(),
                )
            })?;
            if total > crate::sweep::plan::MAX_CONFIG_BYTES {
                return Err(Error::Input(
                    "generated sweep configurations exceed 32 MiB; select a smaller batch; no setup files were written".into(),
                ));
            }
            references.push(format!("{group}/{filename}"));
            configurations.push((filename, text));
        }
        let manifest = toml::to_string_pretty(&crate::sweep::SweepFile {
            resources: None,
            configs: references,
        })
        .map_err(|error| Error::Input(format!("cannot render sweep: {error}")))?;
        if manifest.len() as u64 > crate::sweep::MAX_SWEEP_BYTES {
            return Err(Error::Input(
                "generated sweep manifest exceeds 16 MiB; shorten the output filename or select a smaller batch; no setup files were written".into(),
            ));
        }
        Ok(Self {
            directory,
            configurations,
            manifest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> BenchmarkConfig {
        BenchmarkConfig {
            planning: None,
            agent: "pi".into(),
            workload: "bug-fix".into(),
            provider: "openrouter".into(),
            model: "fixture-model".into(),
            concurrency: 1,
            trial_set: None,
            trial: None,
            workspace: "workspace".into(),
            log_file: "benchmark.log".into(),
            measurements: None,
        }
    }

    #[test]
    fn scalar_serialized_bound_accepts_exact_limit_and_rejects_one_more_byte() {
        let mut config = config();
        config.workspace.clear();
        let overhead = render(&config).unwrap().len();
        config.workspace = "x".repeat(crate::benchmark::MAX_CONFIG_BYTES - overhead);
        assert_eq!(
            render(&config).unwrap().len(),
            crate::benchmark::MAX_CONFIG_BYTES
        );
        config.workspace.push('x');
        assert!(render(&config).unwrap_err().to_string().contains("64 KiB"));
    }

    #[test]
    fn escaped_values_are_bounded_after_toml_encoding_not_by_character_count() {
        let mut config = config();
        config.workspace = "\n".repeat(40_000);
        // TOML may choose a multiline literal rather than escaping every byte;
        // measure its real serialization rather than assuming one encoding.
        let bytes = toml::to_string_pretty(&config).unwrap().len();
        assert_eq!(
            render(&config).is_ok(),
            bytes <= crate::benchmark::MAX_CONFIG_BYTES
        );
        config.workspace = "\u{0000}".repeat(12_000);
        assert!(config.workspace.len() < crate::benchmark::MAX_CONFIG_BYTES);
        assert!(render(&config).unwrap_err().to_string().contains("64 KiB"));
    }

    #[test]
    fn late_oversized_configuration_is_rejected_before_any_setup_publication() {
        let good = config();
        let mut invalid = good.clone();
        invalid.workspace = "x".repeat(crate::benchmark::MAX_CONFIG_BYTES);
        let error = match Sweep::new(Path::new("unused/selection.toml"), vec![good, invalid]) {
            Ok(_) => panic!("oversized late configuration was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("no setup files were written"));
    }

    #[test]
    fn serialized_aggregate_budget_is_checked_before_manifest_publication() {
        let mut config = config();
        config.workspace = "x".repeat(40_000);
        let error = match Sweep::new(Path::new("unused/selection.toml"), vec![config; 840]) {
            Ok(_) => panic!("oversized aggregate was accepted"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("32 MiB"), "{error}");
    }
}
