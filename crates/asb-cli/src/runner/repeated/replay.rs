//! Run-owned whole-track selection and private JSONL staging; no proxy changes.
use super::{Plan, ReplayInput, Slot, path_text, private_directory, publish_bytes};
use crate::cassette::{Compatibility, Limits, SelectionPlan, Selector, Source};
use crate::{Error, benchmark::BenchmarkConfig, runner::RunOptions};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize)]
pub(super) struct BatchChoices {
    pub seed: Option<u64>,
    pub algorithm: Option<String>,
    pub tracks: Vec<u64>,
}

/// Own the opened source until all selected entries have been verified/staged.
/// Legacy no-selector replay deliberately retains its historical routing.
pub(super) struct Prepared {
    source: Source,
    path: PathBuf,
    chosen: Vec<u64>,
    pub selection: Option<SelectionPlan>,
    pub compatibility: Option<Compatibility>,
}

impl Prepared {
    pub fn open(
        config_path: &Path,
        config: &BenchmarkConfig,
        options: RunOptions<'_>,
        repeat: u64,
    ) -> Result<Option<Self>, Error> {
        if options.mode != Some(crate::tape::Mode::Replay) {
            return Ok(None);
        }
        let path = super::super::replay_cassette(config_path, config, options.cassette)?;
        if !Source::is_bundle(&path).map_err(input_error)?
            && options.track.is_none()
            && options.replay_seed.is_none()
        {
            return Ok(None);
        }
        let source = Source::open(&path, Limits::default()).map_err(input_error)?;
        let (chosen, selection, compatibility) = if let Some(manifest) = source.manifest() {
            // Selection is precomputed once in slot order, never by worker arrival.
            let seed = options.replay_seed.unwrap_or_else(generated_seed);
            let selection = SelectionPlan::build(
                manifest,
                repeat,
                config.concurrency as u64,
                seed,
                options.track,
                super::MAX_EXECUTIONS as usize,
            )
            .map_err(input_error)?;
            (
                selection.tracks.clone(),
                Some(selection),
                Some(manifest.compatibility.clone()),
            )
        } else {
            if options.replay_seed.is_some() {
                return Err(Error::Input(
                    "--seed requires a ZIP bundle; legacy JSONL has no completeness catalogue"
                        .into(),
                ));
            }
            let Some(Selector::Id(id)) = options.track else {
                return Err(Error::Input("legacy JSONL supports numeric --track only; statistic observations are unknown".into()));
            };
            if !source.legacy_tracks().iter().any(|track| track.id == id) {
                return Err(Error::Input(format!(
                    "legacy cassette track {id} does not exist"
                )));
            }
            (
                vec![id; (repeat * config.concurrency as u64) as usize],
                None,
                None,
            )
        };
        Ok(Some(Self {
            source,
            path: std::path::absolute(path).map_err(crate::io_error)?,
            chosen,
            selection,
            compatibility,
        }))
    }

    pub fn inputs(&self, slots: &mut [Slot]) -> Result<Vec<ReplayInput>, Error> {
        if slots.len() != self.chosen.len() {
            return Err(Error::Input(
                "replay selection population differs from execution plan".into(),
            ));
        }
        let mut seen = BTreeMap::new();
        let mut inputs = Vec::new();
        let mut total = 0u64;
        for (slot, id) in slots.iter_mut().zip(&self.chosen) {
            let index = if let Some(index) = seen.get(id) {
                *index
            } else {
                let (path, bytes, sha256) = if let Some(manifest) = self.source.manifest() {
                    let track = manifest.track(*id).map_err(input_error)?;
                    (self.path.clone(), track.bytes, track.sha256.clone())
                } else {
                    let track = self
                        .source
                        .legacy_tracks()
                        .iter()
                        .find(|track| track.id == *id)
                        .ok_or_else(|| Error::Input("selected legacy track disappeared".into()))?;
                    (track.path.clone(), track.bytes, track.sha256.clone())
                };
                total = total
                    .checked_add(bytes)
                    .filter(|bytes| *bytes <= Limits::default().max_total_bytes)
                    .ok_or_else(|| {
                        Error::Input("selected replay inputs exceed aggregate byte budget".into())
                    })?;
                let index = inputs.len();
                inputs.push(ReplayInput {
                    track: Some(*id),
                    source: path,
                    private: PathBuf::from(format!("inputs/track-{id}.jsonl")),
                    bytes,
                    sha256,
                });
                seen.insert(*id, index);
                index
            };
            slot.replay_input = Some(index);
            slot.replay_track = Some(*id);
        }
        Ok(inputs)
    }

    pub fn stage(&mut self, root: &Path, inputs: &[ReplayInput]) -> Result<(), Error> {
        private_directory(&root.join("inputs"), false)?;
        for input in inputs {
            // One bounded payload at a time, and only unique selected IDs.
            let bytes = self
                .source
                .read_track(input.track.expect("selected input identity"))
                .map_err(input_error)?;
            if bytes.len() as u64 != input.bytes || crate::cassette::digest(&bytes) != input.sha256
            {
                return Err(Error::Input(
                    "selected cassette changed after planning; no agent was started".into(),
                ));
            }
            publish_bytes(&root.join(&input.private), &bytes)?;
        }
        Ok(())
    }

    pub fn batch_choices(&self, plan: &Plan<'_>, batch: u64, concurrency: usize) -> BatchChoices {
        let start = (batch as usize - 1) * concurrency;
        BatchChoices {
            seed: self.selection.as_ref().map(|selection| selection.seed),
            algorithm: self
                .selection
                .as_ref()
                .map(|selection| selection.algorithm.clone()),
            tracks: plan.slots[start..start + concurrency]
                .iter()
                .map(|slot| slot.replay_track.expect("planned replay slot"))
                .collect(),
        }
    }
}

/// Each batch has its own replica aliases; each proxy still owns its own cursor.
pub(super) fn stage_batch(
    root: &Path,
    plan: &Plan<'_>,
    batch: u64,
    concurrency: usize,
) -> Result<String, Error> {
    let base = root.join(format!("batch-{batch}/replay.jsonl"));
    let start = (batch as usize - 1) * concurrency;
    for slot in &plan.slots[start..start + concurrency] {
        let input = &plan.replay_inputs[slot.replay_input.expect("planned replay input")];
        let destination = if concurrency == 1 {
            base.clone()
        } else {
            crate::benchmark_paths::replica_path(&base, slot.replica)
        };
        fs::hard_link(root.join(&input.private), destination).map_err(crate::io_error)?;
    }
    fs::File::open(base.parent().expect("batch parent"))
        .and_then(|file| file.sync_all())
        .map_err(crate::io_error)?;
    path_text(&base)
}

fn generated_seed() -> u64 {
    // Reproducibility, not a security token: disclose and persist this seed.
    let identity = crate::workloads::TaskIdentity::local("replay-selection-seed", None);
    let hash = Sha256::digest(identity.attempt_id.as_bytes());
    u64::from_le_bytes(hash[..8].try_into().expect("SHA-256 prefix"))
}

fn input_error(error: crate::cassette::Error) -> Error {
    Error::Input(error.to_string())
}
