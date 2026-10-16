#!/usr/bin/env python3
"""Turn AutoUML's implementation identifiers into a stable ASB diagram.

AutoUML intentionally uses source-path-derived identifiers.  They are useful
to a parser but make published diagrams noisy and can expose checkout names.
This post-processor keeps AutoUML as the source of classes and dependency
edges, gives every node a stable local identifier, and adds the small set of
Rust trait implementation edges which AutoUML does not currently emit.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


CLASS = re.compile(r"^\s*class\s+([A-Za-z0-9_-]+)")
EDGE = re.compile(r"^(\s*)([A-Za-z0-9_-]+)(\s+[-.]+[>|]\s+)([A-Za-z0-9_-]+)(\s*)$")
RAW_PREFIX = "asb-cli_src_"
TRAITS = {"AgentAdapter", "ToolAdapter", "WorkloadAdapter", "CollectorAdapter"}


def display_name(raw: str) -> str:
    """Map a generated source identifier to a stable logical Rust name."""
    if not raw.startswith(RAW_PREFIX):
        return raw
    name = raw.removeprefix(RAW_PREFIX)
    for prefix, namespace in (
        ("workloads_workload_adapters_", "workloads::adapters::"),
        ("agents_", "agents::"),
        ("tools_", "tools::"),
    ):
        if name.startswith(prefix):
            return namespace + name.removeprefix(prefix)
    return name


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: normalize_uml.py INPUT.mmd OUTPUT.mmd")
    source, destination = map(Path, sys.argv[1:])
    raw_lines = source.read_text(encoding="utf-8").splitlines()
    blocks: dict[str, list[str]] = {}
    current: str | None = None
    for line in raw_lines:
        if match := CLASS.match(line):
            current = match.group(1)
            blocks.setdefault(current, [])
        elif current is not None:
            if line.strip() == "}":
                current = None
            else:
                blocks[current].append(line)
    classes = sorted(blocks)
    ids = {raw: f"C{index:03d}" for index, raw in enumerate(classes, start=1)}
    names = {raw: display_name(raw) for raw in classes}

    result: list[str] = ["classDiagram"]
    for current in classes:
        label = names[current]
        if label.rsplit("::", 1)[-1] in TRAITS:
            label = f"«trait» {label}"
        result.append(f'    class {ids[current]}["{label}"] {{')
        for line in blocks[current]:
            for raw, logical in names.items():
                line = line.replace(raw, logical)
            result.append(line)
        result.append("    }")

    edges: set[tuple[str, str, str]] = set()
    for line in raw_lines:
        if match := EDGE.match(line):
            start, relation, end = match.group(2), match.group(3), match.group(4)
            if start in ids and end in ids:
                edges.add((start, relation, end))
    for start, relation, end in sorted(edges):
        result.append(f"    {ids[start]}{relation}{ids[end]}")

    implementation_edges = (
        ("asb-cli_src_agents_StaticAdapter", "asb-cli_src_agents_AgentAdapter"),
        ("asb-cli_src_tools_StaticTool", "asb-cli_src_tools_ToolAdapter"),
        (
            "asb-cli_src_workloads_workload_adapters_StaticExternalAdapter",
            "asb-cli_src_workloads_workload_adapters_WorkloadAdapter",
        ),
        (
            "asb-cli_src_workloads_workload_adapters_PromptAdapter",
            "asb-cli_src_workloads_workload_adapters_WorkloadAdapter",
        ),
        ("asb-cli_src_StaticCollector", "asb-cli_src_CollectorAdapter"),
    )
    for implementation, trait in sorted(implementation_edges):
        if implementation in ids and trait in ids:
            result.append(f"    {ids[implementation]} ..|> {ids[trait]}")

    destination.write_text("\n".join(result) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
