#!/usr/bin/env python3
"""Provider-free checks for the build-time function diagrams."""
from pathlib import Path
import shutil
import subprocess
import unittest
import xml.etree.ElementTree as ET

from generate_function_flows import Function, render


class FlowLayoutTests(unittest.TestCase):
    def test_clustered_cycles_keep_every_explicit_relation(self):
        functions = [Function(f"F{index}", f"function_{index}", f"module_{index % 3}",
                              "fixture.rs", index + 1, "{}", True) for index in range(12)]
        relations = {(f"F{index}", f"F{(index + offset) % 12}", "call")
                     for index in range(12) for offset in (1, 4)}
        for kind in ("dataflow", "control"):
            diagram = render(functions, relations, kind)
            self.assertIn("concentrate=false", diagram)
            self.assertNotIn("concentrate=true", diagram)
            self.assertIn("compound=false", diagram)
            self.assertIn("newrank=false", diagram)
            for source, target, _ in relations:
                self.assertEqual(diagram.count(f"    {source} -> {target} ["), 1)
            if shutil.which("dot"):
                result = subprocess.run(["dot", "-Tsvg"], input=diagram, text=True,
                                        capture_output=True, timeout=15)
                self.assertEqual(result.returncode, 0, result.stderr)
                root = ET.fromstring(result.stdout)
                groups = root.findall(".//{http://www.w3.org/2000/svg}g")
                self.assertEqual(sum(group.get("class") == "edge" for group in groups), len(relations))

    def test_real_project_layout_renders_with_bounded_runtime(self):
        if not shutil.which("dot"):
            self.skipTest("Graphviz is an optional build dependency")
        # This larger clustered graph reproduced Graphviz 2.43's crash. Keep
        # the actual project regression in CI, not merely a tiny similar graph.
        from generate_function_flows import collect, edges
        functions = collect(Path(__file__).resolve().parents[1] / "crates")
        relations = edges(functions)
        for kind in ("dataflow", "control"):
            result = subprocess.run(["dot", "-Tsvg"], input=render(functions, relations, kind),
                                    text=True, capture_output=True, timeout=45)
            self.assertEqual(result.returncode, 0, result.stderr)
            root = ET.fromstring(result.stdout)
            groups = root.findall(".//{http://www.w3.org/2000/svg}g")
            self.assertEqual(sum(group.get("class") == "edge" for group in groups), len(relations))
            self.assertEqual(sum(group.get("class") == "node" for group in groups), len(functions))

    def test_control_endpoints_annotate_real_functions_without_fake_calls(self):
        functions = [Function("A", "start", "first", "fixture.rs", 1, "{}", True),
                     Function("B", "finish", "last", "fixture.rs", 2, "{}", True)]
        diagram = render(functions, {("A", "B", "finish")}, "control")
        self.assertIn('A [color="#16a34a", penwidth=2]', diagram)
        self.assertIn('B [color="#dc2626", penwidth=2]', diagram)
        self.assertNotIn("entry ->", diagram)
        self.assertNotIn("-> exit", diagram)


if __name__ == "__main__":
    unittest.main()
