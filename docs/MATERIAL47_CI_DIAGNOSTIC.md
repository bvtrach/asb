# Material47 packaged-CI failure: diagnostic checkpoint

PR316 run39724198680 passed ordinary tests but failed extracted-package library
tests:1234 passed, one failed,13 existing opt-ins. The failing test was
runtime_inputs::tests::nested_probe_inherited_writer_settles_under_outer_gate.
Its outer watchdog reported a budget failure after actual owned-tree cleanup,
but discarded child stdout/stderr and used the same message for a timed-out
child and a child assertion failure. The transcript therefore does not establish
which condition occurred. No timeout increase or product repair is inferred.

This test-only diagnostic retains child output in private create-new0600 files,
without changing the150ms discovery allowance or two-second watchdog. After
confirmed fixture/sentinel settlement it distinguishes actual child status and
returned marker, emits at most16KiB per output on failure and preserves that
failed fixture directory. Success retains normal fixture cleanup. No provider,
privilege, production API or cleanup assertion changes. Source review and fresh
actual-FIRST local qualification precede any hosted diagnostic; the original
failed CI log remains under asb-delivery-pr316/target/pr316-first-hosted.log.

Independent review found that earlier marker/descendant assertions and diagnostic
read failures could still erase evidence before the new reporting branch.
Post-settlement checks now collect all those original obligations together,
preserve the root before diagnostic reads, and report read errors explicitly.
Missing helper/descendant evidence is still failure, never inferred success.
This does not claim new custody guarantees for failures before gate settlement.
