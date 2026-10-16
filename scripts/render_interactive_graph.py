#!/usr/bin/env python3
"""Embed a generated Graphviz SVG in the reusable interactive HTML template."""

from __future__ import annotations

import html
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 7:
        raise SystemExit("usage: render_interactive_graph.py TEMPLATE CSS JS INPUT.svg OUTPUT.html TITLE")
    template, css, js, svg, output = map(Path, sys.argv[1:6])
    title = sys.argv[6] if len(sys.argv) > 6 else "ASB graph"
    rendered = template.read_text(encoding="utf-8")
    rendered = rendered.replace("__TITLE__", html.escape(title))
    rendered = rendered.replace("__CSS__", css.read_text(encoding="utf-8"))
    rendered = rendered.replace("__JS__", js.read_text(encoding="utf-8"))
    rendered = rendered.replace("__SVG__", svg.read_text(encoding="utf-8"))
    output.write_text(rendered, encoding="utf-8")
    print(f"Generated {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
