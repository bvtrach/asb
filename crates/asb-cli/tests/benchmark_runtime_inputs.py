"""Provider-free native runtime closure and non-execution regressions."""
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
ASSETS = Path(__file__).parents[1] / "assets/benchmarks"
sys.path.insert(0,str(ASSETS))
import asb_runtime_inputs as subject
import asb_fingerprint as fingerprints


def native_image(name):
    text = b"\0"+name.encode()+b"\0"
    start = 64+3*64
    symbols = start+len(text)
    data = bytearray(symbols+48)
    data[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<Q",data,40,64)
    struct.pack_into("<HH",data,58,64,3)
    struct.pack_into("<I",data,64+64+4,3)
    struct.pack_into("<QQ",data,64+64+24,start,len(text))
    struct.pack_into("<I",data,64+128+4,11)
    struct.pack_into("<QQ",data,64+128+24,symbols,48)
    struct.pack_into("<I",data,64+128+40,1)
    struct.pack_into("<Q",data,64+128+56,24)
    data[start:symbols] = text
    struct.pack_into("<I",data,symbols+24,1)
    struct.pack_into("<H",data,symbols+24+6,1)
    return data


class RuntimeInputTests(unittest.TestCase):
    def test_python_entries_are_distinct_from_bundled_dependent_libraries(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            module,library = root/"module.abi3.so",root/"libdependent.so"
            module.write_bytes(native_image("PyInit_module"))
            library.write_bytes(native_image("library_function"))
            self.assertTrue(subject.python_extension(module))
            self.assertFalse(subject.python_extension(library))
            self.assertEqual(subject.asb_runtime.library_paths("\tstatically linked\n"),{})
            with self.assertRaises(subject.asb_runtime.ExecutionError):
                subject.asb_runtime.library_paths("statically linked\nunexpected text")
            with patch.object(subject.asb_runtime,"elf",return_value=(b"",62,None)), \
                 patch.object(subject.asb_runtime,"resolve_dependencies",return_value={"dependent":str(library)}) as resolve:
                discovery = subject.Discovery()
                discovery.python_extensions(root,"/fixture-loader")
                self.assertEqual(resolve.call_count,1)
                self.assertEqual(resolve.call_args.args[1],module)
                self.assertIn(str(library),discovery.paths)

    def test_native_symbol_tables_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory)/"module.so"
            for change in ["sections","width","strings","symbols"]:
                image = native_image("PyInit_module")
                if change=="sections": struct.pack_into("<Q",image,40,len(image)+1)
                elif change=="width": struct.pack_into("<H",image,58,1)
                elif change=="strings": struct.pack_into("<Q",image,64+64+32,17*1024**2)
                else: struct.pack_into("<Q",image,64+128+32,25*1024**2)
                file.write_bytes(image)
                with self.assertRaises(subject.InputError,msg=change): subject.python_extension(file)

    @unittest.skipUnless(sys.platform=="linux" and shutil.which("cc"),"needs the local Rust linker")
    def test_real_loader_binds_private_library_without_running_application(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root/"must-not-run"
            (root/"library.c").write_text("int fixture_value(void) { return 0; }\n")
            (root/"application.c").write_text('#include <stdio.h>\nextern int fixture_value(void);\nint main(void) { FILE *f=fopen("'+str(marker)+'","w"); if(f) fclose(f); return fixture_value(); }\n')
            subprocess.run(["cc","-shared","-fPIC",root/"library.c","-o",root/"libfixture.so"],check=True,capture_output=True,timeout=15)
            subprocess.run(["cc",root/"application.c","-L"+str(root),"-lfixture","-Wl,-rpath,"+str(root),"-o",root/"program"],check=True,capture_output=True,timeout=15)
            paths = subject.discover([{"kind":"program","path":str(root/"program"),"search":None}])["paths"]
            self.assertIn(str(root/"libfixture.so"),paths)
            self.assertFalse(marker.exists())
            roots = [{"name":str(index),"path":path} for index,path in enumerate(paths)]
            before = fingerprints.Snapshot(b"k"*32).seal(roots,None)["digest"]
            library = root/"libfixture.so"
            library.write_bytes(library.read_bytes()+b"changed runtime bytes")
            self.assertNotEqual(before,fingerprints.Snapshot(b"k"*32).seal(roots,None)["digest"])
            self.assertFalse(marker.exists())

    @unittest.skipUnless(sys.platform=="linux","uses native ELF interpreters")
    def test_new_earlier_path_candidate_changes_declared_interpreter(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first,second = root/"first",root/"second"
            first.mkdir(); second.mkdir()
            shutil.copy2("/bin/true",second/"fixture-interpreter")
            script = root/"application"
            script.write_text("#!/usr/bin/env fixture-interpreter\nnot an executable application body\n")
            script.chmod(0o700)
            request = [{"kind":"program","path":str(script),"search":os.pathsep.join([str(first),str(second)])}]
            before = subject.discover(request)["paths"]
            self.assertIn(str(second/"fixture-interpreter"),before)
            shutil.copy2("/bin/true",first/"fixture-interpreter")
            after = subject.discover(request)["paths"]
            self.assertIn(str(first/"fixture-interpreter"),after)
            self.assertNotEqual(before,after)

    def test_bounds_and_errors_do_not_expose_private_values(self):
        with self.assertRaises(subject.InputError): subject.discover([{}]*4097)
        with self.assertRaises(subject.InputError): subject.discover([{"kind":"unknown"}])
        with self.assertRaises(subject.InputError): subject.Discovery(timeout=-1).file("/private-value")
        request = [{"kind":"program","path":"/missing/SECRET_RUNTIME_VALUE","search":None}]
        script = "import sys;sys.path.insert(0,"+repr(str(ASSETS))+ ");import asb_runtime_inputs as m;raise SystemExit(m.main())"
        result = subprocess.run([sys.executable,"-I","-S","-B","-c",script],
            input=json.dumps(request).encode(),capture_output=True,timeout=5)
        self.assertNotEqual(result.returncode,0)
        self.assertEqual(result.stderr,b"")
        self.assertNotIn(b"SECRET",result.stdout)
        self.assertIn("error_class",json.loads(result.stdout))


if __name__=="__main__": unittest.main()
