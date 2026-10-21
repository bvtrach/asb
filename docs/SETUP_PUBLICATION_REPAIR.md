# Setup publication repair

Source implementation is focused-qualified below; final independent review,
current-stack integration and delivery qualification remain pending.

An actual current-main CLI reproduction accepted and wrote a 70,443-byte
configuration, then `asb run` rejected that same file because the reader's
limit is 64 KiB. The owned reproduction and setup/run logs are retained under
`asb-evidence-publication/target/matrix-budget-repros/candidate.sm7k6X`.
No provider, installed client or benchmark ran in that reproduction.

The first source phase shares the single-configuration, aggregate input and
manifest bounds with their consuming modules. It serializes every generated
configuration and the final manifest into a bounded prepared sweep before
creating any output directory. Scalar rendering also checks its serialized
size, including TOML escaping. Offline tests cover exact/one-byte-over scalar
bounds, escaping, a late oversized selection and aggregate overflow.

Independent read-only agent review approved the narrow preparation/opened-reader
source at `82f9bc11` / `c3a532cf`, but identified that expanding all cloned
configurations before byte checks could consume GiBs before rejection. Follow-up
`2bd28cae` introduces a shared population accumulator in both CLI and TUI
selection loops, checking each candidate before retaining it and rechecking final
serialization after output-specific paths are assigned. A near-64-KiB population
test proves rejection before hundreds of oversized selections can be retained.
That checkpoint still materialized each individual TOML candidate before its
encoded bound was checked; it did not qualify bounded serializer source storage.

Focused qualification at frozen `2bd28cae`: actual binary built first; config
library filter and actual CLI setup/selection/offline/TUI integration targets
passed, as recorded in `target/setup-population-{config-tests,cli-tests}.log`.
Strict all-target/all-feature Clippy passed (`target/setup-population-clippy.log`).
Actual executable hashes are recorded in the matching before/after SHA files.
The earlier initial Clippy failure (two unnecessary references) remains in
`target/setup-preparation-clippy.log`, with its signed correction and clean
rerun retained. This is focused partial evidence, not full qualification.

The remaining requirements recorded at that partial checkpoint were:

- Preserve an existing scalar configuration on write, sync or publication
  failure; do not truncate it in place.
- Publish the final sweep manifest atomically with no replacement, only after
  every bounded configuration is synchronized. Do not expose partial manifests.
- Track exact owned entries and settle partial setup without recursive cleanup
  of unknown arrivals. Return a useful retained-directory location and distinguish
  pre-commit failure from published-but-durability-unconfirmed presentation.
- Test short writes, Interrupted, WriteZero, ENOSPC, sync and rename failures,
  competing creators, prior bytes/mode and namespace replacements. Cover actual
  CLI setup and the shared TUI publishing route, not just pure serialization.
- Add exact/one-byte-over aggregate/manifest bounds, deterministic opened-file
  growth/replacement tests and timeout-protected FIFO regression; actual CLI/TUI
  tests must prove late size errors write nothing and valid outputs round-trip.
- Complete independent review, fresh full offline qualification, lint/build/docs,
  PR CI, merge, both post-merge jobs and a separately coordinated patch release.

No version slot is assigned. No 90% coverage threshold is introduced. This phase
does not change record/replay, grades, provider/agent adapters or host policy.

## Final source contract, independent review pending

Scalar setup writes and synchronizes a private sibling file before atomically
replacing its destination entry. The permissions of an existing regular file
are preserved as observed before preparation. Precommit write/sync/rename
failure leaves prior bytes and mode unchanged; destination symlinks are replaced,
never followed. Scalars intentionally overwrite; sweeps intentionally do not.

The shared CLI/TUI sweep route creates every configuration with create-new,
writes through its owned descriptor, and synchronizes every configuration,
the generation directory and its parent before linking the manifest. Compact
identity/metadata witnesses and a bounded byte comparison revalidate generated
files without holding up to 65,536 descriptors. The complete synced manifest is
atomically hard-linked with no replacement, so a competing creator wins without
being overwritten and a partial manifest is never visible. After commit, the
destination parent is synchronized. An explicit publication state distinguishes
precommit failure from committed-but-durability/settlement-unconfirmed errors;
confirmation-output failure also clearly says publication occurred.

Every failed sweep generation is deliberately retained, including partial
configurations and unknown arrivals, with its actual directory in the error.
There is no recursive cleanup. Scalar/manifest staging instead retains a live
file and directory identity owner. Before committing or settling, the exposed
entry is atomically detached into a new private namespace, checked against the
owner, and only then used. Foreign files get no-clobber hard-link recovery where
possible; quarantine evidence survives even when another creator blocks recovery.
The exposed directory is detached and checked before nonrecursive removal too.
Foreign directories are retained with their exact paths rather than
blindly renamed over another arrival. Explicit failure has no Drop retry;
unwinding can retain uncommitted staging. Only original staging names, never the
quiet commit names, enter the private fault seam; production has no configurable
fault hook or subprocess callback there.

Before the TOML serializer runs, a schema-specific allocation-free lower-bound
check bounds raw string bytes plus string cardinality for every configuration
field, including optional planning. Exhaustive destructuring and primitive type
assertions force schema additions/type changes to review this check. The pinned
writer preserves decoded UTF-8 bytes or expands them with escaping; even a
multiline string adds delimiters rather than compressing its content. Thus this
lower bound cannot reject a valid <=64 KiB pretty-TOML configuration. The exact
encoded bound is still checked afterward. Manifest-reference storage is likewise
budgeted during construction, before full manifest serialization. This bounds
the source data passed to these concrete serializers; it is not a generic
streaming serializer, an at-most-64-KiB RAM claim, or a budget on already-supplied
CLI/project input allocations.

These are per-file and manifest-last guarantees, not a transaction over all
configurations, newly-created parent directories or unrelated trial-set files.
Precommit observations do not freeze generation files against arbitrary later
writers. Quiet custody assumes the private commit namespace is not handed to
other processes; it is not a hostile same-UID or mutable-parent sandbox. Windows
uses complete native file IDs and file synchronization, but std directory
durability is not claimed there and no Windows runtime qualification is supplied.

## Fresh focused qualification

Built the actual ASB binary first, then passed the configuration library filter
(66), TUI library filter (44), complete `compare_setup` (16), `setup_selection`
(3), `offline_cli` (94 passed, five explicit optional skips) and actual
`tui_terminal` (2). These are focused invocations, not disjoint whole-crate
coverage counts. Strict all-target/all-feature Clippy and formatting passed.

The tests include exact/+1 scalar, aggregate 32 MiB and manifest 16 MiB bounds;
multiline/escaping and optional-planning/cardinality storage; deterministic
growth/replacement after opened-file metadata; and a FIFO reader in a killed/
reaped watchdog subprocess so removing NONBLOCK cannot hang CI. Actual CLI
late-candidate and scalar errors write no setup files/preserve old bytes/mode;
normal output round-trips through comparison. The real TUI PTY reaches final
publication with a 70 KiB workspace, refuses without creating any setup/staging
entry, preserves prior generations byte-for-byte and restores terminal mode.
Valid TUI generations still round-trip, including concurrency 64.

Real owned-file faults cover partial/short/Interrupted/WriteZero/ENOSPC writes,
file and directory sync, native rename failure, competing manifest creators,
blocked partial-manifest writing, foreign files/symlinks/empty directories,
recovery collisions, unknown scratch, retained partial generations, unchanged
prior bytes/mode, no Drop retry and truthful postcommit/confirmation errors.
No provider, installed client, credential or host setting is used.

The actual executable hash was unchanged before/after runtime checks and after
the additional TUI library filter:
`5d2d6832abb6b28c2885800fa39e108fb72e116b140f9dba774d091e0c464bde`.
Evidence remains in `target/setup-publication-qualified-{build,config,cli,tui-lib,clippy,fmt}.log`
and matching before/after/final SHA files. Earlier partial-phase logs are kept.
The final no-Drop-retry assertion was requalified with another 66/0 configuration
filter and strict lint/format checks in the matching `config-final`,
`clippy-final`, `fmt-final` logs; `confirmed.sha256` still matches the runtime hash.
The historical Cargo version 0.2.33 is not a reserved delivery slot. Final source
approval, current-main integration, full offline/build/docs qualification,
hosted CI, merge, both post-merge jobs and a coordinated release remain pending.
