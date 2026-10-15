# Development

Keep the project small and offline-buildable after dependencies are installed.
New behavior belongs in one of the three CLI modules and should preserve the
existing `setup` and `run` command forms.

Use the pinned Rust toolchain and run:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo build --locked
```

These checks are offline and deterministic: CI never starts an agent, bridge,
or live provider. Run `asb run` with credentials only as an explicit local
smoke/benchmark operation.

Workspaces, caches, and build outputs stay under `/srv/data/projects`. Do not
put credentials, machine-specific paths, prompts, or provider responses in the
repository. Live runs receive credentials from the agent/provider environment.

Measurement smoke tests are local-only. Use `asb run CONFIG --measurements
DIR` when a host profile is needed; keep `ASB_MEASURE_*` collectors disabled in
CI. Measurement JSON must remain bounded and must not contain credentials,
provider responses, or full agent transcripts.
