# Setup publication repair: partial preparation checkpoint

This is not a completed repair or a delivery qualification.

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
The TOML serializer still materializes each individual candidate before its
encoded bound is checked; do not claim a general bounded-memory serializer.

Focused qualification at frozen `2bd28cae`: actual binary built first; config
library filter and actual CLI setup/selection/offline/TUI integration targets
passed, as recorded in `target/setup-population-{config-tests,cli-tests}.log`.
Strict all-target/all-feature Clippy passed (`target/setup-population-clippy.log`).
Actual executable hashes are recorded in the matching before/after SHA files.
The earlier initial Clippy failure (two unnecessary references) remains in
`target/setup-preparation-clippy.log`, with its signed correction and clean
rerun retained. This is focused partial evidence, not full qualification.

Remaining requirements before this repair can be considered complete:

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
