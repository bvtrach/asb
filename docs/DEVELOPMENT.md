# Development

Keep the project small and offline-buildable after dependencies are installed.
New behavior belongs in one of the three CLI modules and should preserve the
existing `setup` and `run` command forms.

Use the pinned Rust toolchain and run:

```sh
cargo fmt --all -- --check
cargo test --locked
cargo build --locked
make rust-doc
make uml
```

`make rust-doc` runs `cargo doc --locked --no-deps --document-private-items`,
because the CLI implementation modules are intentionally private. `make uml`
installs the pinned AutoUML release under `target/tools/`, regenerates
`docs/asb-type-diagram.mmd`, normalizes AutoUML's source-path identifiers into
stable logical names, and renders `docs/asb-type-diagram.svg`. The normalized
diagram explicitly shows trait implementation edges and never publishes local
paths or branch names. It supplements the maintained architecture overview;
review both when changing public or private type relationships.

These checks are offline: replay tests start the embedded tape proxy and test
clients on loopback, but CI never starts a real agent, bridge, or live provider.
Run `asb run` with credentials only as an explicit local
smoke/benchmark operation.

The opt-in real-agent fixture test launches an installed client against a
synthetic loopback provider with dummy credentials. On Linux, compile it and
run each client in a separate rootless network and PID namespace with only
loopback enabled (replace the executable with the project-local installed binary).
The PID namespace also stops client-launched helper daemons when the test exits:

```sh
cargo test --locked --test real_codex_smoke --no-run
unshare -cfpn --keep-caps sh -c 'ip link set lo up && exec env ASB_REAL_AGENT_ID=codex ASB_REAL_AGENT_BIN=codex ASB_REAL_AGENT_EXECUTABLE=/absolute/path/to/codex cargo test --locked --test real_codex_smoke real_agent_uses_selected_openrouter_endpoint -- --ignored --nocapture'
```

The fixture refuses to run if it detects non-loopback network routes. Set
`ASB_REAL_PROVIDER=anthropic` to exercise the Anthropic Messages interface, or
`ASB_REAL_PROVIDER=cli2key ASB_REAL_MODEL=gpt-6-astra` for the local bridge's
Responses interface. The test uses fixture keys and never needs a live
provider credential. Some clients perform other initialization before sending
the benchmark prompt, so an endpoint probe alone is not a successful run.

Workspaces, caches, and build outputs stay under `/srv/data/projects`. Do not
put credentials, machine-specific paths, prompts, or provider responses in the
repository. Live runs receive credentials from the agent/provider environment.

Measurement smoke tests are local-only. Use `asb run CONFIG --measurements
DIR` when a host profile is needed; keep `ASB_MEASURE_*` collectors disabled in
CI. Measurement JSON must remain bounded and must not contain credentials,
provider responses, or full agent transcripts.
