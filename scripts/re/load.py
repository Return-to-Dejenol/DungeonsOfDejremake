# Loads ghidra-vb3 from <work folder>/ghidra-vb3 and analyses <work folder>/exe/MORDOR.EXE.
import importlib.util


def load(root):
    spec = importlib.util.spec_from_file_location("vb3", root + r"\ghidra-vb3\ghidra_scripts\DisassembleVB3PCode.py")
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    data = open(root + r"\exe\MORDOR.EXE", "rb").read()
    a, err = m.analyze(data, root + r"\ghidra-vb3\ghidra_scripts", [root + r"\exe"])
    if a is None:
        raise SystemExit(err)
    return m, a
