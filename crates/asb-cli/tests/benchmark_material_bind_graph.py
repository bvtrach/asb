"""Pure bind-view planning; real inode equivalence is tested by material_census."""
import copy
from pathlib import Path
import stat
import sys
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).parents[1]/"assets/benchmarks"))
from asb_material import MaterialError
from asb_material_bind_graph import plan


def view(name, source="/owned/source", exclusions=None):
    return {"name":name,"source":source,"identity":[1,7,stat.S_IFDIR],
        "exclusions":exclusions or []}


class BindGraphTests(unittest.TestCase):
    def test_shared_source_uses_union_visible_content_and_retains_each_view(self):
        for left,right,expected in [(["a"],["a/b"],["a/b"]),
                (["a"],[],[]),(["a","b"],["b/c","a"],["a","b/c"]),
                (["a"],["ab"],[])]:
            with self.subTest(left=left,right=right):
                observed = plan([view("main/mount-001",exclusions=left),
                    view("worker/mount-001",exclusions=right)])
                self.assertEqual(len(observed["roots"]),1)
                self.assertEqual(observed["roots"][0]["exclusions"],expected)
                self.assertEqual(observed["bindings"][0]["exclusions"],sorted(left))
                self.assertEqual(observed["bindings"][1]["exclusions"],sorted(right))

    def test_logical_order_does_not_depend_on_source_names_or_input_order(self):
        first = [view("main/mount-001","/owned/z"),view("worker/mount-001","/owned/a")]
        second = [view("worker/mount-001","/owned/z"),view("main/mount-001","/owned/a")]
        a,b = plan(first),plan(second)
        self.assertEqual(a["bindings"],b["bindings"])
        self.assertEqual(a,plan(list(reversed(first))))
        self.assertEqual(a["roots"][0]["path"],"/owned/z")
        self.assertNotIn("/owned",str(a["bindings"]))

    def test_distinct_nested_sources_remain_joint_roots(self):
        observed = plan([view("a/mount-001","/owned/root"),
            view("b/mount-001","/owned/root/nested")])
        self.assertEqual(len(observed["roots"]),2)

    def test_invalid_duplicate_unbounded_and_contradictory_views_refuse(self):
        base = view("a/mount-001")
        invalid = [[base,base], [base]*33, [{}]]
        for field,value in [("source","/outside/../source"),("identity",[1,0,stat.S_IFDIR]),
                ("identity",[True,7,stat.S_IFDIR]),("exclusions",["../hidden"]),
                ("exclusions",["hidden"]*129),("name","../binding"),("name",""),
                ("exclusions",["."]),("exclusions",["a//b"])]:
            changed = copy.deepcopy(base);changed[field] = value;invalid.append([changed])
        changed = view("b/mount-001");changed["identity"][1] += 1
        invalid.append([base,changed])
        for values in invalid:
            with self.subTest(values=values),self.assertRaises(MaterialError):plan(values)
        self.assertEqual(plan([]),{"roots":[],"bindings":[]})


if __name__ == "__main__":unittest.main()
