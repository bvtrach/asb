# Multi-track recording and replay delivery

This document defines the implementation contract and delivery sequence. It is
not a claim that the CLI features below are available before their integration
and qualification PRs merge.

## Feature boundaries

1. Reusable cassette backend: versioned manifest, bounded selective ZIP reader,
   integrity and compatibility checks, deterministic selectors and selection plan.
2. Repeated execution: sequential batches, concurrent replicas, fresh workload
   material and isolated agent state, stable slot identities and complete results.
3. Recording publication: one writer per bundle, independent Deflate entries,
   durable per-track publication/recovery journal, retained incomplete attempts,
   create-new final publication and cleanup after confirmed success.
4. Run/replay integration: legacy JSONL/layout readers, private selected JSONL
   scratch before existing model checks, strict proxy validation, CLI and logs.
5. Sweep/report integration: persisted execution and replay selection plans,
   checkpoint/resume/reconciliation, scheduling budgets, every execution in
   comparison, honest population/attempt and live/replay labels.
6. Offline Ratatui track viewer: sort/filter/inspect, replay controls, terminal
   restoration, help/completions/quickstart and end-to-end qualification.

Each feature receives independent review, the repository-required gates, a
separate PR and a patch release after merging. Integration tests remain extensible
and provider-free, using fake clients and synthetic loopback providers. Tests of
real installed clients or original benchmark environments are explicit local
qualification, not implicit live-provider operations.

## Execution and selection

`--repeat N` means N sequential batches of the configured concurrent replicas.
Default repeat is one; explicit repeated execution uses newly allocated
workspaces even for one batch. The legacy non-repeated single-run workspace
behavior is preserved. Positive counts and their checked product are validated
before allocation. Track ID is `(batch - 1) * concurrency + replica`, one-based;
completion order never changes it. Batch/replica, workload attempt, dispatch
cohort and selected replay track are distinct identities.

An execution owns its workspace, agent runtime/session, log, measurements and
proxy progress. Workload adapters rebuild their own fixture/source material;
the runner never copies an already-mutated workspace or edits user files.
Canonical initial-content fingerprints exclude ephemeral attempt paths and are
checked across executions. Baseline provenance is retained separately: an
upstream definition is not a claim of measured initial failing tests.

Random replay selects whole complete nonempty traces uniformly with replacement
within a matching compatibility identity. Selection is precomputed in stable
configuration/batch/replica order, using a disclosed seed and versioned algorithm,
and saved before launch. Resume reuses that exact plan. Fixed or statistic
selection repeats the same selected trace for each execution; each execution
still receives a private proxy cursor. Numeric selection may explicitly select
an incomplete trace, but strict request matching/full consumption still apply.

Statistics selectors use complete nonempty tracks with known values. Tokens are
normalized total input plus output (cached input is a subset, not added again).
Duration is execution-only elapsed milliseconds, not preparation or evaluation.
Requests count recorded inference calls, excluding readiness probes. Steps use
the owning agent adapter's counter. Missing observations remain null. Sort by
metric then ID; min/max ties choose lowest ID, median chooses the lower middle.

## Bundle and failure contract

Version one is a ZIP with bounded `manifest.json` and independently Deflate-
compressed `tracks/ID.jsonl` entries. The manifest binds agent/provider/model,
workload/task/dataset identity, prompt and initial material fingerprints. These
are local consistency checks, not authentication or production trust chains.
Repeat/concurrency/measurement choices do not define replay compatibility.

Completeness, process outcome and workload outcome are independent. Failed
attempts may have complete selectable traces; process exit zero or nonempty
JSONL alone never proves completeness. Recording finalization must account for
in-flight requests and interrupted streams. Every attempted slot remains
inspectable, including empty or incomplete traces and unknown metrics.

ZIP finalization is not crash recovery. An owned partial directory contains
immutable completed track payloads and a durable journal; only one writer updates
it. The final archive is created privately and published without replacing an
existing destination. Interrupted publication retains the partial directory.
Ordinary execution failure still finalizes results. Uncompressed payloads are
removed only after successful publication; no unrelated paths are removed.

Readers bound archive size, entry count, manifest size, individual and aggregate
uncompressed size, reject unsupported/encrypted entries, duplicate/unexpected
names and unsafe file kinds, and verify selected payload length/checksum.
Manifest inspection does not decompress all tracks. Request/response inspection
loads only the selected entry and strips terminal control effects when displayed.
Corruption is an actionable data error, not a reason to introduce a signing or
key-management prerequisite.

## Integration invariants and acceptance

- No archive or selection dispatch inside provider adapters or proxy protocol.
- Offline inspection starts no provider, agent, environment or credential setup.
- Replay re-executes real tools; matching material is not a machine snapshot or
  a guarantee of deterministic external state. Reports disclose replay mode.
- Shared batch preparation fails before launching that batch; individual failures
  are collected without suppressing later executions, with a final failure exit.
- Sweep receipts/reservations and comparison enumerate the saved execution slots,
  not just `1..=concurrency`. Duplicate attempts and overwritten slots are errors.
- Repeated attempts never become an official one-attempt-per-task score.
- Replay input bundles enter declared-input seals; recording outputs enter
  completion evidence. Publishing later tracks must not invalidate sealed
  earlier execution evidence: immutable track identities/payloads are the unit.
- Test batch counts/isolation/fresh material/user preservation, baseline mismatch,
  complete failed traces, seed scheduling independence and fixed selection.
- Test ZIP round trips/selective reads/checksums/limits, corrupt archives, disk
  failures, interrupted recovery and overwrite refusal; retain legacy replay tests.
- Test sweep aggregation/resume/reconciliation, null statistics/ties, viewer
  navigation and terminal restoration on normal/error/cancel paths.
- Run fmt, strict Clippy, tests, build, Rust docs, UML, packaging and hosted CI.
