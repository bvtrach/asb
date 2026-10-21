# Step 2: Python SDK harness extraction

Reference download URLs and source-bundle metadata are also owned by the
registered workload adapters. The installer consumes registry hooks instead
of a parallel benchmark table or a Terminal-Bench name switch. Regression tests
bind the complete ordered catalog, source URL digest and legacy bundle metadata,
so moving ownership cannot silently change installation inputs.

`asb_harness.py` owns the infrastructure shared by the original Terminal-Bench
and Harbor adapters: SDK stdout redirection, private JSON output, durable
create-new verifier-start records, and original Docker network capture,
validation and restoration. Harbor no longer imports the TB1 adapter. Task
parsing, build-input filtering, environment construction and official grading
remain with their owning SDK adapters. The existing helpers retain their
behavior and metadata formats, including the non-retry verifier-start marker.

The shared controller support publisher embeds the new helper alongside its
other Python assets. This covers execution preflight and temporary support
copies used by retained inspection/grading without depending on a source
checkout or an installed TB1 adapter. A Rust regression imports the actual
published Harbor bundle in isolated Python with the TB1 module absent.

Harbor resolves workload identity from the bounded catalog or an explicit
`--workload` profile, then runs the existing full pinned catalog verification.
The sole qualified profile remains `terminal-bench-2.1`: original source
revision, population, source manifest, SDK version, single-step Linux task
shape and binary reward contract are unchanged. Task/evaluation identity and
retained runtime verification use the selected identity. Retained grading
continues to bind the original catalog, SDK, definition, captured environment,
immutable service IDs, non-retry marker and original verifier outputs.

This extraction does not register arbitrary Harbor datasets or establish
general Harbor support. Another profile needs pinned catalog provenance,
task/environment compatibility, reward semantics, retained-grading contracts
and its own qualification. Rust descriptor/selection changes belong to step 1.

Offline regressions are registered in `benchmark_protocol`: `benchmark_harness.py`,
`terminal_catalog.py`, `terminal_execution.py`, `harbor_execution.py`, and the
existing retained grading/reconstruction suites. Run them with a project-local
temporary directory, for example:

```sh
mkdir -p target/step2-tmp
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
# Set CARGO_TARGET_DIR to the shared project-local target directory when needed.
TMPDIR="$PWD/target/step2-tmp" cargo test --locked --offline --test benchmark_protocol
TMPDIR="$PWD/target/step2-tmp" cargo test --locked --offline --lib published_harbor_assets
```

These are provider-free SDK seams and asset-publication checks. They do not
requalify live Docker/SDK environments or full upstream benchmark populations.
