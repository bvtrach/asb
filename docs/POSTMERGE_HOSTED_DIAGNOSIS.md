# Main a32 hosted failure diagnosis (not a repair or qualification)

Exact base a32db3a1152509afb454df50e10b043bbf0ae3f7, version0.2.44.
AGENTS and instruction files match the fully read c725 DEVELOPMENT/ARCHITECTURE.
Typed48, installer and delivery worktrees/caches remain untouched. No product
change, build or runtime has occurred in this diagnostic checkpoint.

Run39711305901 job119603026328 used Ubuntu26.04 image20261018.168.1 and failed
five library tests (1206passed/5failed/13ignored). Complete original transcript:
/home/martin/projects/asb-delivery-pr311/target/pr311-postmerge-hosted-failed.log.

1. launch/tests.rs174 copies /bin/echo to a non-UTF8 basename. The actual
   returned bytes say `<unknown binary name> 0.10.0 (multi-call binary)` with
   an applet list rather than the requested raw argument. This is consistent
   with a multicall dispatcher selecting its applet from the renamed argv0,
   not proof that ASB changed the original command. A candidate test-only fix
   retains basename echo beneath a non-UTF8 parent directory, preserving an
   actual non-UTF8 executable path and argument; the explicit non-UTF8 arg0
   shell case stays unchanged. Controlled dispatcher evidence is required first.
2. perfetto.rs1331 and runtime_inputs.rs483 fail actual runtime discovery with
   the bounded public helper-failed classification. It deliberately omits raw
   helper stderr and does NOT establish the underlying Python exception or
   failing dependency. The former probes a copy of /bin/true named unshare;
   the latter combines current Process, Python(site=false), shebang script and
   a missing optional file. Do not assume either is merely a fixture problem.
3. runner_phases.rs2576 cannot read one of plan.json, benchmark.toml or started
   receipt; sweep_phases.rs51 cannot read an expected JSON file. Input sealing
   calls runtime discovery before initial snapshot construction and can be
   unavailable independently of grades. A shared upstream helper failure could
   explain missing optional seals/receipts, but the failed log does not prove
   this link. Missing artifacts are not permission to weaken these assertions.

Proposed bounded diagnostic phase, subject to independent approval: copy ONLY a
verified-idle owned HOME cache into this new project's own target; build actual
ASB first, freeze/hash and run the five exact owning filters without source edits.
Run a project-local driver that loads the unchanged production Python Discovery
definitions without invoking the public catch/redaction wrapper, then tests each
hardcoded owned-fixture dependency independently. Record operation category and
exception class first; any full diagnostic traceback stays in the owned private
fixture log, never in public errors. Use a finite subprocess timeout and actual
owned subprocess custody/cleanup for the harness; no live provider/SDK/network,
privileges, host settings, real credential values or inherited-input logging.
Keep the original limits, shell environment policy and generic orchestration.
No blind CI retry, applet policy expansion, raw error disclosure, skip or budget
increase is proposed. If local host cannot reproduce image-specific behavior,
report that limit and request a narrow owned diagnostic in the actual hosted
environment rather than guessing a production change. New source and design
review precede any repair implementation or public promotion.

Image source: https://github.com/actions/runner-images/blob/ubuntu26/20261018.168/images/ubuntu/Ubuntu2604-Readme.md
