# Reading MORDOR.EXE

Tools for reading the original game's VB3 p-code, used to recover its exact rules
(docs/balance.md "Original rules"). They work on a local copy only; never commit their
output, which is derived from the original program.

1. Make a work folder `W` outside the repo with:
   - `W/exe/`: MORDOR.EXE and VBRUN300.DLL (and the .VBX files) from your copy.
     MORDOR.WIP is a zip; `vbrun300.dl_` expands with `expand.exe`.
   - `W/ghidra-vb3/`: https://github.com/dfl/ghidra-vb3 (MIT). In
     `ghidra_scripts/DisassembleVB3PCode.py`, `_propagate_byref_types` loops forever on
     this program: change `changed = True` there to `changed = changed or bool(b.type)`.
   - `W/tools/`: these three scripts.
2. `python -I W/tools/decomp.py W W/mordor.bas` writes pseudo-VB for every procedure
   (about 2 seconds). For the notation, see the header of decomp.py and
   docs/formats.md "Reading MORDOR.EXE".
3. `python -I W/tools/rectype.py W <descriptor hex>` prints a record type's fields, for
   example `eac` for the character record.
