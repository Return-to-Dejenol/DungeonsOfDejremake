# Rebuilds VB-like statements from ghidra-vb3's token records (stack simulation).
# Usage: python -I decomp.py <work folder> <out file>. Fields print as .@<offset><type
# suffix>; module constants as mXXXX{value}; one-line Ifs close with "End If '(single line)".
import sys, re, struct
sys.path.insert(0, sys.argv[1] + r"\tools")
from load import load

m, a = load(sys.argv[1])
d, subs = a["disasm"], a["subs"]
_g = subs.globals
_raw = _g.data[_g.base:_g.base + _g.size]
_SD = _raw[_raw.find(struct.pack("<8h", *_g.some_data[:8])):][:len(_g.some_data) * 2]

def _u(o):
    return struct.unpack_from("<H", _SD, o)[0] if o + 2 <= len(_SD) else None

def _isdesc(o):
    t = _u(o + 4)
    return _u(o + 2) in (o + 8, o + 0x19, 0) and t is not None and _u(o + 6) is not None and (t < 9 or t >= 0x100)

SUF = {1: "%", 2: "&", 3: "!", 4: "#", 5: "@", 7: "$", 8: "$"}
NAMES = {}   # (type tag, offset) -> name, filled in by hand as fields are identified

def field(op):
    if not _isdesc(op):
        return f"p{op:X}"
    t, off = _u(op + 4), _u(op + 6)
    return NAMES.get(op, f"@{off}") + SUF.get(t, "") + ("[]" if _u(op + 2) == op + 0x19 else "")

BIN = {"+", "-", "*", "/", "\\", "^", "Mod", "=", "<>", "<", ">", "<=", ">=", "And", "Or", "Xor", "Eqv", "Imp", "&"}
CONV = {"Integer": "CInt", "Long": "CLng"}

def vname(r):
    if r.get("callee") is not None:
        return r["callee"].name or "fn?"
    if r.get("ref"):
        return r["ref"]
    v = r.get("var")
    if v is not None:
        if v.storage == "module" and not v.array:
            w = _g.module_word(v.module, v.slot)
            if w is not None:
                return f"{v.name}{{{w if w < 0x8000 else w - 0x10000}}}"
        return v.name or r.get("ann") or "?"
    return r.get("ann") or "?"

def seg_base(off):
    for s in a["segments"]:
        if s["file_off"] <= off < s["file_off"] + s["file_size"]:
            return s["file_off"]
    return 0

def const(r, kw):
    p = r["params"]
    if kw == "c!" and len(p) >= 4:
        return repr(struct.unpack("<f", p[:4])[0])
    if kw == "c#" and len(p) >= 8:
        return repr(struct.unpack("<d", p[:8])[0])
    if kw == "c@" and len(p) >= 8:
        return repr(struct.unpack("<q", p[:8])[0] / 10000)
    return str(r.get("cval"))

def proc_text(recs):
    out, st = [], []
    blocks = []   # open Ifs: [target]; other blocks: ["For"], ["Do"], ["Select"]
    base = seg_base(recs[0]["off"])
    def pop():
        return st.pop() if st else "?"
    def emit(s, dl=0):
        out.append("  " * (len(blocks) + dl) + s)
    for r in recs:
        kw = (r.get("kw") or "").strip()
        tok, off = r["tok"], r["off"]
        while blocks and isinstance(blocks[-1], int) and blocks[-1] <= off:
            blocks.pop(); emit("End If  '(single line)")
        w = m._words(r)
        c = d.convert_token(tok) or {}
        if r["str"] is not None:
            s = r["str"].decode("latin1") if isinstance(r["str"], bytes) else str(r["str"])
            st.append('"' + s + '"')
        elif kw in ("c%", "c&", "c!", "c#", "c@", "&Hc%", "&Hc&"):
            st.append(const(r, kw))
        elif kw in ("True", "False"):
            st.append(kw)
        elif kw in ("nl", "eos", "eol", "eom", "fin", "exe", ""):
            if kw in ("nl", "eos") and st:
                emit("' leftover: " + " | ".join(st)); st.clear()
        elif kw == "var":
            st.append(vname(r))
        elif kw == "var()":
            n = w[0] if w else 0
            args = [pop() for _ in range(n)][::-1] if n < 0x8000 else []
            st.append(f"{vname(r)}({', '.join(args)})")
        elif kw == "pop.var":
            o = pop(); st.append(f"{o}.{field(w[0])}" if w else f"{o}.?")
        elif kw == "pop.var()":
            o = pop(); args = [pop() for _ in range(w[0])][::-1]
            st.append(f"{o}.{field(w[1])}({', '.join(args)})")
        elif kw == "var=":
            emit(f"{vname(r)} = {pop()}")
        elif kw == "var()=":
            args = [pop() for _ in range(w[0])][::-1]
            emit(f"{vname(r)}({', '.join(args)}) = {pop()}")
        elif kw == "pop.var=":
            o = pop(); emit(f"{o}.{field(w[0])} = {pop()}")
        elif kw == "pop.var()=":
            o = pop(); args = [pop() for _ in range(w[0])][::-1]
            emit(f"{o}.{field(w[1])}({', '.join(args)}) = {pop()}")
        elif kw == "C<typ>":
            t = m.COERCION_TARGETS.get(tok)
            if t in CONV and st:
                st[-1] = f"{CONV[t]}({st[-1]})"
        elif kw in BIN:
            alt = d.flag_token[tok // 3] if tok // 3 < len(d.flag_token) else 0
            if kw == "-" and alt & 0x1FF == m.ALT_UNARY_MINUS:
                st.append("-" + pop()); continue
            b, x = pop(), pop(); st.append(f"({x} {kw} {b})")
        elif kw == "Not":
            st.append(f"Not {pop()}")
        elif kw == "call":
            n = w[0] if w else 0
            args = [pop() for _ in range(n)][::-1] if n < 0x8000 else []
            emit(f"Call {vname(r)}({', '.join(args)})")
        elif kw == "If":
            emit(f"If {pop()} Then"); blocks.append(base + w[0] if w else off + 2)
        elif kw in ("ElseIf", "Else"):
            if blocks and isinstance(blocks[-1], int):
                blocks.pop()
            emit(f"{kw} {pop()} Then" if kw == "ElseIf" else "Else")
            blocks.append(base + w[0] if w else off + 2)
        elif kw == "End If":
            if blocks and isinstance(blocks[-1], int):
                blocks.pop()
            emit("End If")
        elif kw == "Select Case":
            emit(f"Select Case {pop()}"); blocks.append("Select")
        elif kw == "End Select":
            if blocks: blocks.pop()
            emit("End Select")
        elif kw.startswith("Case") or kw.startswith("Is"):
            n = max(c.get("bit_fwd2", 0), 0)
            args = [pop() for _ in range(n)][::-1]
            label = kw[4:].strip() if kw.startswith("Case") else kw
            emit(f"Case {label} {' To '.join(args)}".rstrip(), -1)
        elif kw == "For":
            args = st[:]; st.clear()
            emit("For " + (f"{args[0]} = {args[1]} To {args[2]}" + (f" Step {args[3]}" if len(args) > 3 else "") if len(args) >= 3 else " ; ".join(args)))
            blocks.append("For")
        elif kw == "Next":
            if blocks: blocks.pop()
            emit("Next " + " ".join(st)); st.clear()
        elif kw in ("Do", "Do While", "Do Until"):
            emit(f"{kw} {pop() if kw != 'Do' else ''}".rstrip()); blocks.append("Do")
        elif kw in ("Loop", "Loop While", "Loop Until"):
            if blocks: blocks.pop()
            emit(f"{kw} {pop() if kw != 'Loop' else ''}".rstrip())
        else:
            n = c.get("bit_fwd2", 0); fl = c.get("flags", 0)
            if n < 0:
                args = st[:]; st.clear()
            else:
                args = [pop() for _ in range(n)][::-1]
            if fl & 8 or (fl & 4 and n == 0 and kw != "Me"):
                st.append(f"{kw}({', '.join(args)})" if args or kw == "Rnd" else kw)
            elif kw == "Me":
                st.append("Me")
            else:
                emit(f"{kw} {', '.join(args)}".rstrip())
    return out

with open(sys.argv[2], "w", encoding="latin1") as f:
    for label, i, recs in a["procs"]:
        sub = subs.proc_sub(recs)
        name = subs.declaration(sub) if sub else f"{label}_proc_{i}"
        txt = "\n".join(proc_text(recs))
        mm = re.search(r"occurred in (\w+):(\w+)", txt)
        f.write(f"\n'==== {recs[0]['off']:#08x} {name}" + (f"  [{mm.group(1)}.{mm.group(2)}]" if mm else "") + "\n")
        f.write(txt + "\n")
