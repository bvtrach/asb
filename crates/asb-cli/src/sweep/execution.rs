//! One isolated configuration, independent of scheduling and workload identity.
use super::{
    Options, benchmark_stem, cassette_filename, checked_benchmark_reference, isolated_config, plan,
    reuse_replay_inputs, spool,
};
use crate::{Error, runner, tape};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub(super) fn result_name(index: usize, reference: &Path) -> String {
    format!("{index:03}-{}", benchmark_stem(reference))
}

pub(super) fn replay_sources(plan: &plan::SweepPlan, root: &Path) -> Vec<PathBuf> {
    plan.entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            root.join(result_name(index, Path::new(&entry.reference)))
                .join("cassettes")
        })
        .collect()
}

pub(super) struct Execution<'a> {
    pub source: &'a str,
    pub parent: &'a Path,
    pub plan: &'a plan::SweepPlan,
    pub results: &'a Path,
    pub backup: &'a Path,
    pub replay_inputs: Option<&'a Path>,
    pub restore_recordings: bool,
    pub options: Options<'a>,
    pub warnings: &'a Mutex<BTreeSet<String>>,
    pub checkpoint: Option<&'a super::checkpoint::Inputs>,
    pub reused: &'a super::resume::State,
}

impl Execution<'_> {
    pub(super) fn config_path(&self, index: usize) -> PathBuf {
        self.parent.join(&self.plan.entries[index].reference)
    }

    fn directory(&self, index: usize) -> PathBuf {
        self.results
            .join(result_name(index, &self.config_path(index)))
    }

    pub(super) fn output_path(&self, index: usize) -> PathBuf {
        // Keep temporary presentation bytes outside the immutable task result
        // tree. Evidence hashing must not omit similarly named grader files.
        self.results
            .join(format!(".terminal-output-{index:06}.tmp"))
    }

    pub(super) fn run(&self, index: usize) -> Result<(), Error> {
        std::fs::create_dir_all(self.directory(index)).map_err(crate::io_error)?;
        if self.options.resume {
            spool::preserve(
                &self.output_path(index),
                self.plan.entries[index].configuration.concurrency,
                &self.results.join(".checkpoint/interrupted-terminal"),
            )?;
        }
        let mut output = spool::Output::create(
            &self.output_path(index),
            self.plan.entries[index].configuration.concurrency,
        )?;
        let result = self.run_to(index, &mut output);
        let flush = output.flush().map_err(crate::io_error);
        match (result, flush) {
            (Err(error), Err(flush)) => {
                Err(Error::Run(format!("{error}; output flush failed: {flush}")))
            }
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        }
    }

    /// Sequential defaults retain immediate warnings and --show-output.
    pub(super) fn run_to(&self, index: usize, output: &mut dyn Write) -> Result<(), Error> {
        if let Some(summaries) = self.reused.groups.get(&index) {
            return super::resume::render_reused(summaries, self.options.json, output);
        }
        let entry = &self.plan.entries[index];
        let config = &entry.configuration;
        let config_path = checked_benchmark_reference(self.parent, &entry.reference, self.source)?;
        let result_dir = self.directory(index);
        std::fs::create_dir_all(&result_dir).map_err(|error| {
            Error::Run(format!(
                "cannot create results directory {}: {error}",
                result_dir.display()
            ))
        })?;
        if self.options.mode == Some(tape::Mode::Replay) && self.restore_recordings {
            let previous = self
                .backup
                .join(self.results.file_name().expect("results name"))
                .join(result_name(index, &config_path))
                .join("cassettes");
            reuse_replay_inputs(
                &previous,
                &result_dir.join("cassettes"),
                config,
                self.options.resume,
            )?;
        }
        let resolved = result_dir.canonicalize().map_err(|error| {
            Error::Run(format!(
                "cannot resolve isolated benchmark {}: {error}",
                result_dir.display()
            ))
        })?;
        let isolated = isolated_config(config, &resolved);
        let isolated_path = resolved.join("benchmark.toml");
        let serialized = toml::to_string_pretty(&isolated)
            .map_err(|error| Error::Run(format!("cannot render sweep benchmark: {error}")))?;
        if !self.options.resume || !isolated_path.exists() {
            std::fs::write(&isolated_path, serialized).map_err(|error| {
                Error::Run(format!("cannot write {}: {error}", isolated_path.display()))
            })?;
        } else if crate::read_config(&isolated_path)? != serialized {
            return Err(Error::Input("isolated sweep configuration changed; preserve the existing attempts before starting a fresh sweep".into()));
        }
        let cassette = self.replay_inputs.map(|root| {
            root.join(result_name(index, &config_path))
                .join("cassettes")
                .join(cassette_filename(config))
        });
        let observer = self
            .checkpoint
            .map(|inputs| super::checkpoint::ReplicaObserver {
                inputs,
                index,
                directory: &resolved,
                entry,
                publication: Mutex::new(()),
            });
        if !self.reused.slots.contains_key(&index)
            && let Some(observer) = observer.as_ref()
        {
            observer.reserve()?;
        }
        let result = runner::run_with_diagnostics(
            isolated_path.to_string_lossy().as_ref(),
            runner::RunOptions {
                json: self.options.json,
                show_output: self.options.show_output,
                measurement_dir: None,
                mode: self.options.mode,
                cassette: cassette.as_deref().and_then(Path::to_str),
                respect_record_timings: self.options.respect_record_timings,
                on_completion: observer
                    .as_ref()
                    .map(|observer| observer as &dyn runner::replica::CompletionObserver),
                reused: self.reused.slots.get(&index),
                ..Default::default()
            },
            output,
            self.warnings,
        );
        // Preserve official false grades too. Failure to produce a complete
        // evidence seal must not change the benchmark result or retry a task.
        if let Some(checkpoint) = self.checkpoint {
            let _ = checkpoint.completed(index, &resolved, entry);
        }
        result
    }
}
