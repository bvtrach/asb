#!/usr/bin/env python3
"""Render the AutoUML Mermaid class diagram as a deterministic offline SVG.

The source diagram remains Mermaid for editing and documentation tooling. This
small renderer deliberately needs only Python's standard library, so `make
build` also produces an inspectable SVG on hosts without Node/Chromium.
"""

from __future__ import annotations

import html
import re
import sys
from pathlib import Path


IDENTIFIER = r"[A-Za-z0-9_-]+"
CLASS = re.compile(rf"^\s*class\s+({IDENTIFIER})")
RELATION = re.compile(rf"^\s*({IDENTIFIER})\s+[-.]+[>|]\s+({IDENTIFIER})")


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: render_mermaid_svg.py INPUT.mmd OUTPUT.svg")
    source, destination = map(Path, sys.argv[1:])
    nodes: list[str] = []
    edges: list[tuple[str, str]] = []
    for line in source.read_text(encoding="utf-8").splitlines():
        if found := CLASS.match(line):
            if found.group(1) not in nodes:
                nodes.append(found.group(1))
        elif found := RELATION.match(line):
            edges.append(found.groups())
    columns, width, box_width, box_height = 4, 1120, 250, 82
    rows = max(1, (len(nodes) + columns - 1) // columns)
    height = 110 + rows * 118
    positions = {
        node: (40 + (index % columns) * 270, 70 + (index // columns) * 118)
        for index, node in enumerate(nodes)
    }
    lines = [
        '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %d %d">'
        % (width, height, width, height),
        '<title>ASB Rust type diagram</title>',
        '<defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="#4f5d75"/></marker></defs>',
        '<rect width="100%" height="100%" fill="#f8fafc"/>',
        '<text x="40" y="38" font-family="sans-serif" font-size="24" font-weight="700" fill="#172033">ASB Rust type diagram</text>',
        '<text x="40" y="58" font-family="sans-serif" font-size="13" fill="#526277">Generated from docs/asb-type-diagram.mmd by AutoUML</text>',
    ]
    for start, end in edges:
        if start not in positions or end not in positions:
            continue
        x1, y1 = positions[start]
        x2, y2 = positions[end]
        lines.append(
            '<line x1="%d" y1="%d" x2="%d" y2="%d" stroke="#64748b" stroke-width="1.25" marker-end="url(#arrow)"/>'
            % (x1 + box_width // 2, y1 + box_height // 2, x2 + box_width // 2, y2 + box_height // 2)
        )
    for node in nodes:
        x, y = positions[node]
        label = html.escape(node.replace("_", " "))
        lines.extend(
            [
                '<rect x="%d" y="%d" width="%d" height="%d" rx="8" fill="#ffffff" stroke="#2563eb" stroke-width="1.5"/>'
                % (x, y, box_width, box_height),
                '<text x="%d" y="%d" font-family="sans-serif" font-size="15" font-weight="600" fill="#172033">%s</text>'
                % (x + 14, y + 38, label),
            ]
        )
    lines.append("</svg>")
    destination.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
