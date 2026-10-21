# Raw capture custody integration (staged 0.2.36)

Source-only integration combines independently reviewed/focused raw repair
`bd2dea5e` with artifact/publication stack `b25d9f11`. It keeps the exact raw
implementation and its true/false record-to-selected-replay fixture, while
retaining the incoming shared provider fixture and all publication/artifact
fault cases. No collector/runtime/installer/setup repair is folded in.

Patch 0.2.35 belongs to artifact custody; its full qualification is running.
This new 0.2.36 tree is not qualified by either historical parent's tests.
Fresh combined focused/full qualification, independent integration review,
build/lint/docs, PR CI, merge and both post-merge jobs are required before a
signed release. The removed coverage threshold remains removed.
