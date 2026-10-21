# Remaining code-smell audit: 21 October 2026

Read-only baseline: exact main `4224472f5fccb3a5651593fff8fbfd8f0b557a60`
versus independently qualified stack
`48609d5083d000b43ce8ba2e4eebcd8b553a40f2`. Remote main advanced to
`73f3ce4768ef9f6dec06a25884c19228683bbe26` during the audit; later delivery
is not attributed to the earlier main baseline. The original 52-line report
remains the scope, not just the next repair. This is source evidence, not a
claim that all repairs or runtime qualifications are complete.

M below means implemented on that exact main; Q means additionally present in
the qualified, then-unmerged stack. Tests named here were inspected; the audit
itself ran no builds, tests, containers, providers or host mutations.

## Full requirement/disposition matrix

| Report requirement | Disposition and source/test evidence |
| --- | --- |
| Runtime endpoint `unreachable!` | M: `agents::StaticAdapter::prepare` checks advertised interface/authentication pairs and adapters return sanitized errors. Registry endpoint matrix covers rejected and advertised pairs; Claude's ninth cited site was a test. |
| Runner god function | M consuming admission/preparation/execution/settlement/evaluation/finalization states repair grading/cleanup ordering (`runner/phases.rs`, actual `runner_phases` fixtures). Preparation/publication still have responsibilities; publication and artifact ownership below are actual gaps, not line-count objections. |
| Comparison god functions | M loaded/validated/prepared/published phases, `RowId`/`ResultSource`, opened-once bounded reads, atomic generation publication and rollback/uncertain-commit tests (`compare/*`, `tests/unit/report_pipeline.rs`). |
| Sweep orchestration | Q consuming Prepared/Fresh-or-Resumed/Activated/Settled states, sticky presentation, typed worker stop, exact spool ownership and partial comparison; nine actual replay-only cases plus held-sibling scheduler tests. Main baseline retains sequential JSON admission and early settlement gaps. |
| Perfetto configuration 630-line claim | Misreport: small configuration builder, not a demonstrated extraction need. |
| BPF scope validator 533-line claim | Misreported size; actual multiline/OR/mutation/startup defects repaired on M with positive scope proof, frozen source and owned startup fixtures. |
| Remote execution orchestration | `workloads/remote.rs` has named operation budgets and explicit evaluation/recovery contracts on M. Receive/decode/metadata/cancellation decomposition remains useful; do not erase retained in-place versus fresh grading semantics. |
| cli2key startup/session | Q bounded readers/readiness, pre-spawn custody, all-child liveness admission and generic explicit provider finish; actual embedded proxy/curl and offline lifecycle fixtures. Not on exact M. |
| Model fetch | M bounded poll/size/EOF/exit transport and reaper custody; Q explicit provider-session finish. |
| Matrix nesting/index panic | `config/matrix.rs` still combines expansion, validation and publication. Empty `configs[0]` is not shown reachable: required selections, nonempty expansion/trial/model guards precede insertion. Existing invalid/zero/oversized CLI tests. Preserve scalar overwrite versus nonoverwriting sweep contracts. |
| Installer recipe | Residual `install_recipe` combines NPM/Python/runtime/environment/host/custom routing. Typed `Recipe` already exists. Prepared recipe inputs and owned publication are useful, not merely extracting five functions. |
| Workload-source four-tuple | Residual private `(source,name,project_root,project_config)` in `install::workload_source`; a resolved-source record belongs with installer phases, not a demonstrated runtime failure alone. |
| Observer/checkpoint error loss | M redacted deduplicated typed diagnostics retain grades and conservative continuation; actual filesystem and broken-writer tests. |
| Model kill/wait error loss | M bounded cleanup and pre-established `OwnedReaper`; do not repeat the repaired class. |
| All ignored cleanup | Not equivalent operations. Consumed artifacts are actionable; unique scratch retention needs honest diagnostics, not necessarily failed grades. Preserved cache/source/evaluator evidence must not be indiscriminately removed. |
| Collector raw consumption before result publication | Extended review confirmed `process_stopped_collectors` deletes stdout/stderr before result JSON publication, losing its complete source on staging or confirmation failure. This class now retains raw captures until confirmed commit and uses opened-file/private-move identity checks. Replacements/removal failures remain inspectable; unrelated adapter-artifact ownership stays separate. |
| Run status/failure strings | Q serialized `RunStatus`, `FailureKind`, `RunFailure` and private `RunDisposition::from_facts`; M independently derives status/success/failure. Twelve actual cases retain pending/native/Boolean semantics. |
| Workload evaluation-kind strings | Adapter extension/historical boundary, not a closed enum. `scored_kind` plus baseline evidence keeps unknown categories display-only/unscored. |
| Saved sweep mode | Q `SavedExecutionMode`: four unchanged strings, missing historical Unknown, null/bad spelling rejected, replay-only timing; wire/fingerprint unchanged. |
| Comparison result source | M private enum; historical status/evaluation strings deliberately tolerant. |
| Private dispatch/controller literals | Residual protocol maintenance, no demonstrated typo defect. Typed decoding/versioning should live with each protocol, not one cross-component mega-enum or incidental wire change. |
| TUI trial suffix identity | Q typed `ChoiceKey` and named panes; labels cannot retarget selection, install or trial identity (`tui/identity_tests.rs`). M retains suffix/positional logic. |
| Path sanitization | M shared `benchmark_paths`; graph allocator separately owns uniqueness. Legacy lossy normalization is not a confinement guarantee. |
| Sudo probes | M typed prerequisite consent/probe policy; fake executor and isolated fake-sudo regressions preserve independent/dependent operations. |
| Three TUI progress workers | Q shared owned helper/gate operation and progress stages, PTY EPIPE/cancel/owner-loss/descendant tests. M retains detached workers. |
| Seven usage summation loops | Claimed equivalence false. Cline final aggregate, Qwen/WorkBuddy final cumulative envelopes, Goose no cited local sum; Pi/OpenCode/OpenJiuwen per-call sums have different cache/completeness rules. `TokenUsage::add`, `usage_fields` and source arbitration already share arithmetic. Existing adapter tests prevent double-counting. |
| References/executables | M shared checked TOML reference and project executable policy. Host discovery intentionally differs from project-local resolution. |
| SWE/Terminal grade/inspect/recover | Residual adapter wiring duplication; actual engines already share `remote::{grade_retained,inspect_grade,verify_retained,recover}`. Catalog/tool/module/evaluation-mode differences are semantic. Narrow official-engine descriptor possible. |
| Checkpoint Boolean/nesting | M validated-outcome constructor with named protocol/cleanup/cohort/identity/grade/path invariants and mutation tests. Preserve validate-all-before-publish. Q sweep consumes validated resumed state. |
| Excess parameters | Pi configure(7), Cline configure(6), cassette selection(6), remote launch(6) remain. Cohesive request records may help; parameter count alone is not a defect. Capture policy belongs with transport ownership. |
| Large result structs | Q disposition owns operational decision; M comparison has typed private phases. Serialization/report records can legitimately be large. Do not change wire fields just to reduce counts. |
| String/error consistency | Residual boundary design; workload/collector strings and crate/cassette errors have different owning contracts. Preserve error categories and primary ordering. |
| Tool preparation global mutex | Necessary protection for dependency publication, not benchmark execution. Per-project ownership may improve concurrency; removal without replacement is unsafe. |
| Network project thread-local | Nested restoring guard and thread-isolation test exist. Explicit context is a possible follow-up, not unsynchronized state. |
| Signal globals | Exclusive gate only; atomic flag/FD and eventfd write are intentional signal-safe ownership. Preserve actual owner-loss fixtures. |
| Atomic unique counters | Not defects; PID/time/counter names plus create-new establish ownership. Do not replace with unsynchronized state. |
| Model limit literal | M curl limit derives from `transport::MAX_RESPONSE`. |
| TUI 16/64 and panes[5] | Q shared maximum/numeric keys/`PaneKind`; M retains both inconsistencies. |
| Runtime-input magic limits | Bounds exist; naming constants is minor. Actual worker/read/join/wait lifetime problem belongs to bounded transport below. |
| Visibility | Private module scope matters: `Root`/`Fingerprint` already pub(super), `RunResult`/comparison row private. Narrow sibling fields where invariants warrant; no blanket external API exposure. |
| Alleged test-only functions | Misreport: guest driver calls `run_guest_process`; completion/compare/sweep use `resolve_log_path`. |
| Prompt dead-code assertion | Redundant `_assert_adapter_contract`; registry coercion already enforces the trait. Small cleanup, not high-priority repair. |
| Naming | `run_mode` naming can improve during owning changes; provider-local implementation modules, tape orchestration and llmtape adapter have distinct roles. |
| Flattened signal exit | Actual gate/remote information loss remains. Needs versioned/backward-compatible termination evidence across gate/guest/controller/result, not local 128+signal substitution. |
| json_string parse claim | Misreport: `serde_json::to_string(&str)` serializes, does not parse. Defensive fallback is unnecessary style, not hidden malformed input. |
| Perfetto metadata shadow | Present, harmless readability issue; rename with nearby work. |
| Reserved all | Selection paths share `selection_names::{ALL,validate}` and registry/catalog tests. mpstat CPU all and BPF tracepoint all are separate protocols; TUI fixture literals do not prove validation drift. |
| Completion duplication | M Clap-derived Bash/Zsh tree and real shell fixtures repair schema drift. |
| CliError Display variants | Categories remain meaningful; owned-string variants already share one arm. No defect from equal display behavior. |
| Clean-area observations | Low-level unsafe, synchronized state, justified invariants and registry macros are not blanket removal tasks. No new TODO/unwrap-count target is inferred. |

## Verified publication risks and coherent next class

`runner::write_log` appends serde fragments and a separate newline directly to
the published file, without writer exclusion. Partial writes corrupt the last
record; a later append can join it into another invalid record. Independent
writers can interleave fragments. `compare::outcomes::load_run` rejects the
latest malformed JSON line. Append does not ordinarily truncate prior bytes;
the problem is record integrity and publication ownership.

`measurements::write_json` uses File::create before serialization; a mid-write
failure can destroy prior run/lifecycle/collector/summary bytes. `update_summary`
silently converts malformed JSON or malformed/missing runs to empty history.
The inspected main/qualified functions are materially identical here. Current
runner fault cases prove blocked-open handling, not partial-write preservation.

Next class: prepared serialization, persistent destination lock, opened regular
snapshot and private same-filesystem stage, atomic per-file commit and truthful
published-but-durability-unconfirmed state. Preserve malformed history rather
than resetting it. Retain complete original bytes, unknown metadata, task and
cohort identity, planning and native/Boolean grades. Independent finalization
must continue; receipt confirmation is separate. Never retry a client to repair
reporting. No multi-file transaction or universal crash-recovery claim.

Extended review also reproduced an unterminated historical record exactly at
the 16 MiB window: adding its missing newline creates an unreadable record.
That case must fail before commit with all historical bytes unchanged. Collector
raw sources must outlive failed/unconfirmed result publication, while sibling
exports and run/lifecycle/summary finalization continue. Opened capture identity
is rechecked after moving a candidate into private custody, before deletion;
a racing foreign entry is restored without replacement or retained with its
exact inspection path. Missing native identity retains scratch explicitly.

Offline tests: short writes, EINTR, WriteZero, ENOSPC, serialization, rename and
sync faults; good-old-file sentinels; malformed historical summary; two real
process writers; symlink/FIFO refusal; one successful publication followed by
another failure. Actual runner entry checks true/false/native/pending outcomes,
broken presentation, exact-once execution/evaluation/cleanup, primary-error
precedence, reports and absent receipts together.

## Adapter artifacts: separate subsequent class

Both runner and guest driver ignore `remove_file` for `AgentArtifacts.cleanup_paths`.
Mini-SWE final response and usage read a fixed workspace trajectory; preparation
does not bind it to the current attempt. An old trajectory can supply the next
attempt's answer/counters when that client emits no replacement, even apart
from deletion failure. Hermes uses a unique run_dir and has different stale
attribution risk. This is not repaired by adding a post-grade `?`.

Prefer private per-attempt Mini-SWE output and attempt-context answer/collection;
otherwise preserve old files before admission and prove current ownership.
Freeze collected evidence, then explicitly settle only owned consumed artifacts
once. Never delete a foreign replacement. Retain grades and safe ancillary
diagnostics despite cleanup/publication failure; distinguish scratch retention
from workload process cleanup. Native/guest fixtures should run A with answer
and usage, then B without output, plus cleanup refusal, replacement/symlink,
publication faults, broken presentation and grade/identity/receipt checks.

## Ordered residual implementation plan

1. Owned post-grade log/measurement publication and historical-summary preservation.
2. Attempt-owned adapter artifacts and consuming guest-driver phases.
3. Generic capture/runtime-discovery transport: bounded queues/bytes, pre-spawn
   workers, cancellation, inherited-pipe/uncertain-child custody. Current runner
   uses unbounded captures/detached readers; after known exit its timeout check
   no longer bounds pipe EOF. Runtime discovery joins blocking workers/waits.
4. Installer recipe publication: Host and Goose copy into final executable
   paths unlike repaired staged source/dependency publication. Preserve existing
   binaries on copy/mode/validation failure, introduce prepared recipe/source
   records and scoped scratch diagnostics; no package-manager rollback claim.
5. Matrix prepared-selection/publication phases with population proof, all
   serialization before writes, nonoverwriting manifest and owned partial-output
   diagnostics. Fault tests, not an invented empty-index panic claim.
6. Narrow official-engine descriptor sharing while retaining dataset, retained
   resource and evaluator semantics.
7. Versioned termination evidence preserving signals without changing grades.
8. Cohesive adapter requests, project-scoped installer locking/explicit context,
   redundant assertions/shadowing and owning protocol decoder cleanup.

Usage sharing remains conservative and lower priority than evidence integrity.
Delivery of earlier queued classes does not close these residual obligations.
