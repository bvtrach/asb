#!/usr/bin/env python3
"""Organize AutoUML's Rust Graphviz output into readable module clusters."""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path


NODE = re.compile(r'^\s*"([^"]+)"\s+\[.*\];\s*$')
EDGE = re.compile(r'^\s*"([^"]+)"\s+->\s+"([^"]+)"\s+\[.*\];\s*$')

MODULES = (
    "agents", "benchmark", "cli2key", "compare", "config", "install",
    "measurements", "models", "prepare", "project", "providers", "runner",
    "suggestions", "sweep", "tape", "tools", "tui", "workloads",
)

# Recent AutoUML releases use short type identifiers for the single-crate
# layout. Keep the ownership explicit for those identifiers so the diagram
# remains useful even when two source files contain a similarly named type.
TYPE_MODULES = {
    "Bridge": "cli2key",
    "ProviderSpec": "providers",
    "ModelKind": "providers",
    "ModelSpec": "providers",
    "RunResult": "runner",
    "CommandSpec": "runner",
    "Availability": "tui",
    "Choice": "tui",
    "Pane": "tui",
    "App": "tui",
    "Generated": "tui",
    "WorkloadApp": "tui",
    "BenchmarkConfig": "benchmark",
    "MeasurementConfig": "benchmark",
    "ProjectConfig": "project",
    "ModelInfo": "models",
    "TrialSet": "workloads",
    "Trial": "workloads",
    "Fixture": "workloads",
    "Mode": "tape",
    "BenchmarkData": "compare",
    "LoggedStatus": "install",
    "Recipe": "install",
    "ExternalWorkload": "install",
    "BundleSpec": "install",
    "TokenUsage": "measurements",
    "RunMeasurement": "measurements",
    "SystemInfo": "measurements",
    "CollectorResult": "measurements",
    "Collector": "measurements",
    "FlamegraphStatus": "measurements",
}


def module_for(node: str) -> str:
    prefix = "asb-cli_src_"
    if node.startswith(prefix):
        rest = node[len(prefix):]
        for module in MODULES:
            if rest == module or rest.startswith(module + "_"):
                return module
        if rest in TYPE_MODULES:
            return TYPE_MODULES[rest]
    return "core"


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: organize_uml_dot.py INPUT.dot OUTPUT.dot")
    source, destination = map(Path, sys.argv[1:])
    nodes: dict[str, str] = {}
    edges: set[tuple[str, str, str]] = set()
    for line in source.read_text(encoding="utf-8").splitlines():
        if match := NODE.match(line):
            # AutoUML double-escapes Graphviz record-label escapes (for
            # example ``\\\\l``). Collapse that layer before rendering.
            nodes[match.group(1)] = line.strip().replace("\\\\", "\\")
        elif match := EDGE.match(line):
            edges.add((match.group(1), match.group(2), line.strip()))

    groups: dict[str, list[str]] = defaultdict(list)
    for node in sorted(nodes):
        groups[module_for(node)].append(node)

    lines = [
        "digraph ASB_Rust_UML {",
        '    graph [bgcolor="white", compound=true, fontname="Helvetica", '
        'fontsize=18, label="ASB Rust architecture", labelloc=t, '
        'newrank=true, nodesep=0.35, pad=0.25, rankdir=LR, ranksep=1.15, '
        'splines=polyline];',
        '    node [color="#64748b", fillcolor="#f8fafc", fontname="Helvetica", '
        'fontsize=9, shape=record, style="filled,rounded"];',
        '    edge [color="#94a3b8", fontname="Helvetica", fontsize=8, arrowsize=0.7];',
        "",
    ]
    for group in sorted(groups):
        cluster_id = re.sub(r"[^A-Za-z0-9_]", "_", group)
        lines.extend([
            f'    subgraph "cluster_{cluster_id}" {{',
            f'        label="{group}";',
            '        color="#cbd5e1";',
            '        fontcolor="#334155";',
            '        style="rounded,dashed";',
        ])
        for node in groups[group]:
            lines.append(f"        {nodes[node]}")
        lines.extend(["    }", ""])

    for start, end, line in sorted(edges):
        lines.append(f"    {line}")
    lines.append("}")
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
