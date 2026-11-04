# Prints a record type's fields from its first field descriptor: python -I types.py <root> <hex op>
import sys, struct; sys.path.insert(0, sys.argv[1] + r"\tools"); from load import load
m, a = load(sys.argv[1])
g = a['subs'].globals; raw = g.data[g.base:g.base + g.size]
SD = raw[raw.find(struct.pack('<8h', *g.some_data[:8])):][:len(g.some_data) * 2]
u = lambda o: struct.unpack_from('<H', SD, o)[0] if o + 2 <= len(SD) else 0
T = {1: 'Integer', 2: 'Long', 3: 'Single', 4: 'Double', 5: 'Currency', 7: 'String', 8: 'String*n'}
o = int(sys.argv[2], 16)
for _ in range(400):
    nxt, t, off = u(o + 2), u(o + 4), u(o + 6)
    arr = nxt == o + 0x19
    tn = T.get(t, f'Type@{t:#x}')
    print(f"{o:#06x} +{off:<5} {tn}" + (f" x{u(o + 8)}" if arr else ''))
    if nxt == 0:
        break
    o = o + 0x18 if arr else nxt
