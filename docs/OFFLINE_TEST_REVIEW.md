# Offline test review

This review targets ASB main at `daafb7ca53218eaf84bdf0739cab14e404e02949`.
The independent review prioritized safety boundaries and end-to-end behavior,
not just the number of tests. A sanitized LLVM run measured 65.79% line
coverage before these changes and 70.90% afterward. These figures include
in-module test code and are not production-only or branch coverage; they are
indicators, not guarantees of behavioral coverage.

The new tests use disposable project directories, fake executables, synthetic
responses, and loopback listeners. CLI subprocesses clear the inherited
environment, set a disposable `HOME`, and use only explicit dummy credentials.
Fake agents validate their endpoint is loopback before sending requests. These
measures do **not** provide operating-system network isolation.

The test additions cover CLI parsing/help, configuration generation, local
source installation, malformed sweeps, concurrent replay, partial-sweep
reports, process outcomes, tape cleanup, model metadata and cassette parsing,
TUI navigation and rendering, and measurement SVGs. Existing integration
tests now clear their environments; an existing TUI test now uses a model
fixture instead of attempting live provider discovery.

## Confirmed defects repaired by this change

The following regression tests are enabled and run in the normal suite:

1. `reset_must_not_delete_an_external_directory_through_a_symlink`: `asb init
   --reset` can delete data outside the project through an intermediate
   symlink in a configured managed directory. Avoid reset on projects with
   custom or untrusted directory settings. Reset now rejects configured paths
   other than direct project children and rejects symlinks before deleting
   state. Move nested managed directories manually and adjust the project
   configuration before retrying. This does not protect against a concurrent
   hostile process changing the project tree during initialization.
2. `cli2key_failed_startup_must_stop_its_mock_bridge`: a bridge which closes
   startup output while still running is not stopped after startup fails.
3. `replay_must_not_expose_a_provider_credential_to_the_agent`: OpenAI replay
   passes the selected credential to the agent. This test uses only a dummy
   string and does not establish that a live provider was contacted. Replay
   now removes known provider and registered agent credential environment
   variables and supplies dummy values where needed. It is not a sandbox: an agent can still access the user's files,
   other environment variables, or the network.
4. `search_with_no_matches_must_not_toggle_a_hidden_choice`: selecting in an
   empty search result toggles a hidden TUI choice.

The reset test creates its external target inside its own disposable fixture;
the bridge fixture exits on its own and never signals a PID from a pidfile.
The normal test suite runs all four regressions.

Two further review findings were confirmed and repaired: the runner now kills
and reaps the owned agent on collector-attachment and output failures, and the
tape rejects oversized request and upstream response bodies while reading them
instead of buffering them first. These are covered by offline process and
loopback tests.

## Remaining priorities

- Expand end-to-end failure injection for individual collector attachment
  failures and their subprocess trees; the agent cleanup guard itself is tested.
- Expand record/replay tests across agent and provider adapters, including
  endpoint overrides, streaming responses, and request/response redaction.
- Add fake-tool collector lifecycle, permission-failure, and artifact tests,
  plus more offline installer idempotence and failure-injection cases.

Some older tape unit tests honor inherited process-level overrides. For a
credential-free test run, use a disposable `HOME`, preserve only the Rust
toolchain and required executable paths, clear provider keys, proxy variables,
collector opt-ins, and `ASB_LLM_TAPE`, and run Cargo in offline mode. Enforce
network restrictions outside the test process if an OS-level guarantee is
required.
