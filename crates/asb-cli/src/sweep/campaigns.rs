//! Sweep-owned campaign preparation/publication; execution stays runner-owned.
use super::{Options, execution, plan};
use crate::{Error, runner, tape};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Prepared {
    entries: Vec<(runner::repeated::Prepared, PathBuf)>,
}

pub(super) struct Pool {
    entries: Vec<Entry>,
}
struct Entry {
    published: Mutex<Option<runner::repeated::PublishedCampaign>>,
    root: PathBuf,
    stop: AtomicBool,
}

fn error(reason: &str) -> Error {
    Error::Input(format!("sweep campaign: {reason}"))
}

pub(super) fn prepare(
    parent: &Path,
    results: &Path,
    preserved: &Path,
    plan: &mut plan::SweepPlan,
    options: Options<'_>,
) -> Result<Option<Prepared>, Error> {
    if options.cassette_root.is_some() && options.mode.is_none() {
        return Err(error("cassette root requires record or replay"));
    }
    if (options.track.is_some() || options.seed.is_some())
        && options.mode != Some(tape::Mode::Replay)
    {
        return Err(error("track and seed require replay"));
    }
    let previous = if options.mode == Some(tape::Mode::Replay) && options.cassette_root.is_none() {
        plan::SweepPlan::source_snapshot(results, parent)?
    } else {
        None
    };
    let sources = plan.entries.iter().enumerate().map(|(index, entry)| {
        let name = execution::result_name(index, Path::new(&entry.reference));
        if options.mode == Some(tape::Mode::Record) {
            // Default bundles outlive results rotation; immutable recording
            // associations must not point into a subsequently moved result tree.
            return Ok(Some(options.cassette_root.map(|root| root.join(&name).join("cassettes/recording.asb.zip"))
                .unwrap_or_else(|| parent.join("cassettes").join(format!("sweep-{}", plan.run_id)).join(name).join("recording.asb.zip"))));
        }
        if options.mode.is_none() { return Ok(None); }
        if let Some(old) = previous.as_ref().and_then(|saved| saved.entries.get(index))
            && let Some(reference) = &old.campaign
        {
            if old.reference != entry.reference { return Err(error("default replay source configuration order changed; select an explicit cassette root")); }
            let layout = runner::campaign::reading::Layout::read_reference(results, reference)?;
            let source = layout.plan.recording.or(layout.plan.replay_source).or_else(|| layout.plan.replay_inputs.first().map(|input| input.source.clone()))
                .ok_or_else(|| error("previous campaign has no recording/replay source"))?;
            return Ok(Some(source));
        }
        let root = options.cassette_root.unwrap_or(results).join(name).join("cassettes");
        let bundle = root.join("recording.asb.zip");
        Ok(Some(if crate::cassette::Source::is_present(&bundle).map_err(|failure| error(&failure.to_string()))? { bundle } else { root.join(super::cassette_filename(&entry.configuration)) }))
    }).collect::<Result<Vec<_>, Error>>()?;
    let bundles = sources
        .iter()
        .map(|source| {
            if options.mode == Some(tape::Mode::Replay) {
                crate::cassette::Source::is_bundle(source.as_deref().expect("replay source"))
                    .map_err(|failure| error(&failure.to_string()))
            } else {
                Ok(false)
            }
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let modern = options.repeat.is_some()
        || options.track.is_some()
        || options.seed.is_some()
        || options.mode == Some(tape::Mode::Record)
        || bundles.iter().any(|bundle| *bundle)
        || previous
            .as_ref()
            .is_some_and(|saved| saved.execution.is_some());
    if !modern {
        return Ok(None);
    }
    if options.resume || options.reconcile_completed {
        return Err(error(
            "campaign continuation needs saved-slot validation; existing results were not changed",
        ));
    }
    let repeat = options.repeat.unwrap_or(1);
    plan.entries
        .iter()
        .try_fold(0u64, |total, entry| {
            repeat
                .checked_mul(entry.configuration.concurrency as u64)
                .and_then(|count| total.checked_add(count))
        })
        .filter(|total| repeat > 0 && *total <= 65_533)
        .ok_or_else(|| error("invalid or excessive total repeat/concurrency population"))?;
    let seed = options.seed.or_else(|| {
        bundles.iter().any(|bundle| *bundle).then(|| {
            let id = crate::workloads::TaskIdentity::local("sweep-replay-seed", None);
            let digest = crate::cassette::digest(id.attempt_id.as_bytes());
            u64::from_str_radix(&digest[..16], 16).expect("canonical digest prefix")
        })
    });
    let mut entries = Vec::new();
    let mut input_bytes = 0u64;
    let mut input_count = 0usize;
    let mut plan_bytes = 0u64;
    for (index, ((entry, source), bundle)) in plan
        .entries
        .iter_mut()
        .zip(&sources)
        .zip(&bundles)
        .enumerate()
    {
        let campaign_parent = results
            .join(execution::result_name(index, Path::new(&entry.reference)))
            .join("campaigns");
        let mut prepared = runner::repeated::Prepared::prepare(
            &parent.join(&entry.reference),
            entry.configuration.clone(),
            repeat,
            runner::RunOptions {
                json: options.json,
                show_output: options.show_output,
                mode: options.mode,
                cassette: source.as_deref().and_then(Path::to_str),
                track: options.track,
                replay_seed: if *bundle { seed } else { options.seed },
                respect_record_timings: options.respect_record_timings,
                policy: entry.policy.as_ref(),
                ..Default::default()
            },
            Some(&campaign_parent),
        )?;
        prepared.preserve_source_locations(results, preserved)?;
        let reference = prepared.reference(results)?;
        let (count, bytes) = prepared.input_cost();
        input_count = input_count
            .checked_add(count)
            .filter(|count| *count <= 256)
            .ok_or_else(|| error("retained replay input population exceeds 256"))?;
        input_bytes = input_bytes
            .checked_add(bytes)
            .filter(|bytes| *bytes <= 2 * 1024 * 1024 * 1024)
            .ok_or_else(|| error("private replay inputs exceed aggregate 2 GiB"))?;
        plan_bytes = plan_bytes
            .checked_add(reference.bytes)
            .filter(|bytes| *bytes <= plan::MAX_PLAN_BYTES as u64)
            .ok_or_else(|| error("campaign plans exceed aggregate 64 MiB"))?;
        entries.push((prepared, results.join(&reference.root)));
        entry.campaign = Some(reference);
    }
    plan.bind_execution(plan::ExecutionIntent {
        repeat,
        seed,
        user_seed: options.seed.is_some(),
        selector: options.track,
    })?;
    Ok(Some(Prepared { entries }))
}

impl Prepared {
    pub fn publish(self, results: &Path) -> Result<Pool, Error> {
        // Collect ALL publications before returning execution authority. A
        // failure drops locks but retains the index and already-published roots.
        let entries = self
            .entries
            .into_iter()
            .map(|(prepared, root)| {
                runner::repeated::publish_campaign(prepared).map(|published| Entry {
                    published: Mutex::new(Some(published)),
                    root,
                    stop: AtomicBool::new(false),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let index = runner::campaign::reading::bounded(
            &results.join("plan.json"),
            plan::MAX_PLAN_BYTES as u64,
        )?;
        crate::evidence::write_json(&results.join("campaigns-published.json"), &serde_json::json!({"schema_version":1,"scope":"published-plans-and-private-inputs-not-attempt-evidence","plan_sha256":crate::cassette::digest(&index),"configurations":entries.len()})).map_err(Error::Io)?;
        Ok(Pool { entries })
    }
}

impl Pool {
    pub fn run(
        &self,
        index: usize,
        output: &mut dyn Write,
        warnings: &Mutex<BTreeSet<String>>,
    ) -> Result<(), Error> {
        let entry = &self.entries[index];
        let published = {
            let mut owned = entry
                .published
                .lock()
                .map_err(|_| error("campaign execution ownership was interrupted"))?;
            owned
                .take()
                .ok_or_else(|| error("campaign was already consumed"))?
        };
        let result = runner::repeated::execute(published, output, warnings);
        let finished =
            runner::campaign::reading::bounded(&entry.root.join("finished.json"), 16 * 1024)
                .and_then(|bytes| {
                    serde_json::from_slice::<serde_json::Value>(&bytes)
                        .map_err(|_| error("malformed campaign settlement"))
                });
        let stop = matches!(result, Err(Error::Input(_) | Error::Io(_)))
            || !finished
                .is_ok_and(|value| value["schema_version"] == 1 && value["stopped"] == false);
        entry.stop.store(stop, Ordering::Release);
        result
    }

    pub fn stop_requested(&self, index: usize) -> bool {
        self.entries[index].stop.load(Ordering::Acquire)
    }
}
