# Text ids for the original's words (docs/formats.md "The original's text"): prints each
# argument's id and whether the imported STRINGS.TXT has it. With --find, lists the
# imported strings containing a word instead (their text stays local).
# Usage: python -I textid.py "Not enough gold!" ...   |   python -I textid.py --find gold
import sys
from pathlib import Path

def fnv(s):
    h = 0x811C9DC5
    for b in s.encode("utf-8"):
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return h

def rust(h):
    return f"0x{h >> 16:04x}_{h & 0xFFFF:04x}"

table = Path(__file__).resolve().parents[2] / "gamedata" / "STRINGS.TXT"
have = set(table.read_text(encoding="utf-8").split("\n")) if table.exists() else set()
if sys.argv[1:2] == ["--find"]:
    for s in sorted(x for x in have if sys.argv[2].lower() in x.lower()):
        print(rust(fnv(s)), repr(s))
else:
    for s in sys.argv[1:]:
        print(rust(fnv(s)), "ok" if s in have else "not in STRINGS.TXT", repr(s))
