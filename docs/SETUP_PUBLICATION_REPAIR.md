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
size, including TOML escaping. Proposed offline tests cover exact/one-byte-over
scalar bounds, escaping, a late oversized selection and aggregate overflow.
At this checkpoint those tests are not yet executed or independently reviewed.

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
- Complete independent review, fresh full offline qualification, lint/build/docs,
  PR CI, merge, both post-merge jobs and a separately coordinated patch release.

No version slot is assigned. No 90% coverage threshold is introduced. This phase
does not change record/replay, grades, provider/agent adapters or host policy.
