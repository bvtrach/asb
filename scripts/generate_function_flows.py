#!/usr/bin/env python3
"""Generate function-level dataflow and control-flow Graphviz diagrams."""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path


FUNCTION = re.compile(
    r"^\s*(?:(?:pub(?:\([^)]*\))?|async|unsafe|extern|default)\s+)*"
    r"fn\s+([A-Za-z_][A-Za-z0-9_]*)"
)
CALL = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")
KEYWORDS = {"if", "for", "loop", "match", "while", " Some", "Ok", "Err"}


@dataclass
class Function:
    ident: str
    name: str
    module: str
    file: str
    line: int
    body: str
    public: bool


def brace_body(lines: list[str], start: int) -> str:
    text = "\n".join(lines[start:])
    opening = text.find("{")
    if opening < 0:
        return ""
    depth = 0
    quote: str | None = None
    escaped = False
    end = len(text)
    for index, char in enumerate(text[opening:], opening):
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
            continue
        if char in ('"', "'"):
            quote = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                end = index + 1
                break
    return text[opening:end]


def collect(source_root: Path) -> list[Function]:
    functions: list[Function] = []
    for path in sorted(source_root.rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        relative = path.relative_to(source_root).with_suffix("")
        module = "::".join(relative.parts)
        for index, line in enumerate(lines):
            match = FUNCTION.match(line)
            if not match:
                continue
            prefix = line[: match.start(0)]
            public = bool(re.search(r"\bpub(?:\([^)]*\))?\b", prefix + line[: match.end(1)]))
            functions.append(
                Function(
                    ident=f"F{len(functions):04d}",
                    name=match.group(1),
                    module=module,
                    file=str(path.relative_to(source_root.parent.parent)),
                    line=index + 1,
                    body=brace_body(lines, index),
                    public=public,
                )
            )
    return functions


def edges(functions: list[Function]) -> set[tuple[str, str, str]]:
    by_name: dict[str, list[Function]] = defaultdict(list)
    for function in functions:
        by_name[function.name].append(function)
    result: set[tuple[str, str, str]] = set()
    for function in functions:
        for name in CALL.findall(function.body):
            if name in KEYWORDS or name == function.name:
                continue
            candidates = by_name.get(name, [])
            same_module = [item for item in candidates if item.module == function.module]
            selected = same_module if len(same_module) == 1 else candidates if len(candidates) == 1 else []
            if selected:
                result.add((function.ident, selected[0].ident, name))
    return result


def escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def render(functions: list[Function], relations: set[tuple[str, str, str]], kind: str) -> str:
    by_id = {function.ident: function for function in functions}
    groups: dict[str, list[Function]] = defaultdict(list)
    for function in functions:
        groups[function.module].append(function)
    lines = [
        f"digraph ASB_{kind.title()}_Flow {{",
        '    graph [bgcolor="white", compound=true, fontname="Helvetica", '
        f'fontsize=18, label="ASB {kind} flow (Rust functions)", labelloc=t, '
        'newrank=true, nodesep=0.18, pad=0.25, rankdir=LR, ranksep=0.8, '
        'splines=polyline, concentrate=true];',
        '    node [color="#64748b", fillcolor="#f8fafc", fontname="Helvetica", '
        'fontsize=8, shape=box, style="filled,rounded"];',
        '    edge [color="#94a3b8", fontname="Helvetica", fontsize=7, arrowsize=0.55];',
        "",
    ]
    for module in sorted(groups):
        cluster = re.sub(r"[^A-Za-z0-9_]", "_", module)
        lines.extend([
            f'    subgraph "cluster_{cluster}" {{',
            f'        label="{escape(module)}";',
            '        color="#cbd5e1";',
            '        fontcolor="#334155";',
            '        style="rounded,dashed";',
        ])
        for function in groups[module]:
            fill = "#dbeafe" if function.public or function.name == "main" else "#f8fafc"
            label = f"{function.name}\\n{function.file}:{function.line}"
            lines.append(
                f'        {function.ident} [label="{escape(label)}", fillcolor="{fill}"];'
            )
        lines.extend(["    }", ""])

    for source, target, _called_name in sorted(relations):
        caller = by_id[source]
        callee = by_id[target]
        style = "solid" if kind == "dataflow" else "dashed"
        label = f"{caller.name}() -> {callee.name}()" if kind == "dataflow" else f"calls {callee.name}()"
        lines.append(f'    {source} -> {target} [label="{label}", style="{style}"];')

    if kind == "control":
        roots = sorted({function.ident for function in functions} - {target for _, target, _ in relations})
        leaves = sorted({function.ident for function in functions} - {source for source, _, _ in relations})
        lines.extend([
            '    entry [label="entry points", shape=oval, fillcolor="#dcfce7"];',
            '    exit [label="terminal functions", shape=oval, fillcolor="#fee2e2"];',
        ])
        for ident in roots:
            lines.append(f'    entry -> {ident} [color="#16a34a", style=dotted];')
        for ident in leaves:
            lines.append(f'    {ident} -> exit [color="#dc2626", style=dotted];')
    lines.append("}")
    return "\n".join(lines) + "\n"


def main() -> int:
    if len(sys.argv) != 4:
        raise SystemExit("usage: generate_function_flows.py SOURCE_ROOT DATAFLOW.dot CONTROLFLOW.dot")
    source_root, dataflow, controlflow = map(Path, sys.argv[1:])
    functions = collect(source_root)
    relations = edges(functions)
    dataflow.write_text(render(functions, relations, "dataflow"), encoding="utf-8")
    controlflow.write_text(render(functions, relations, "control"), encoding="utf-8")
    print(f"Generated {len(functions)} functions and {len(relations)} call edges")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
