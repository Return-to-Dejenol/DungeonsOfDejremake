# Original data formats (MDATA*.MDR)

Worked out from the shareware v1.1 files; the parser is `crates/rules/src/mdr.rs`.
"Probable" means inferred from value patterns, not yet checked against gameplay.

## Container

Each file is a Visual Basic 3 random-access file of fixed-size records. VB3 types only:
Integer (i16), Long (u32/i32), Single (f32), and strings stored as a u16 length then
Latin-1 bytes. Fields are packed with no alignment. A record's fields are shorter
than the record, so the tail of every record is leftover disk junk (file names,
fragments of other records). Ignore anything past the decoded fields.

- Record 0: version string `"1.1"`.
- Next records: Integer counts, written over junk, so only their first 2 bytes count.
- Data records follow. The last record can be cut short at end of file.

| File | Record | Counts | Data |
|---|---|---|---|
| MDATA1 | 260 | rec 1-6: races 9, guilds 12, item types 35, item classes 24, monster types 18, monster families 15 | rec 7 on, tables back to back |
| MDATA2 (spells) | 75 | rec 1: 105 | rec 2-106 |
| MDATA3 (items) | 125 | rec 2: 366 (rec 1 = 55, unknown) | rec 3 on |
| MDATA5 (monsters) | 160 | rec 2: 401 (rec 1 = 7, unknown) | rec 3 on |
| MDATA11 (dungeon) | 20 | rec 0: floors (3 shareware, 15 registered); rec 1-n: 1-based header record of each floor | see below |

The registered release ships MDATA11 outside `MORDOR.WIP`, as `mdata11.md_` packed with
MS-DOS `COMPRESS.EXE` (SZDD); the importer expands it. MDATA11 has no version record.

## Fields (word = i16, offsets after the name)

- **Race**: w0-5 stat minimums (STR INT WIS CON CHA DEX), w6 unused; w7-12 maximums, w13
  unused; w14-25 resistances (order unconfirmed); w26 alignment mask (1 good, 2 neutral,
  4 evil); w27-29 unknown (w29 may be the point-buy budget); w30 lifespan; w31-32 f32 XP
  factor (probable).
- **Guild**: w0-2 unknown (w0 may be HP per level: Warrior 6, Wizard 2); w3-4 f32 XP
  factor (probable; Nomad 8 is lowest); w5 unknown; w6-11 stat requirements; w13
  alignment mask; the rest is mostly f32 tables, not decoded yet.
- **Item**: w0 id; w1-2 probably attack/defense modifiers (negative on cursed items);
  w3-4 u32 price; w5 0-16, maybe level found; w15 guild-usable bitmask (bit n = guild n;
  4095 = all); w23 index into item types.
- **Monster**: w2 id (0-400); w7 level found (1-16; lairs on each floor hold monsters of
  that level, so near certain); w0, w1, w3 large varied values (likely attack, defense,
  hit points); w4 1-4.
- **Spell**: w0 id; w1 0-17; w2 level (probable); w3 0-145, maybe SP cost.

## Dungeon floors (MDATA11)

Each floor is a run of 20-byte records; only the first 12 bytes of each are used.

1. Header: width 30, height 30, level, areas in use, chute slots, teleporter slots.
2. 900 squares, row-major from the south-west corner: Integer area number, then a
   Currency (i64 / 10,000) holding the square flags.
3. Areas: a count record, then 201 slots (`Dim a(200)`). Each is a Long monster-type
   mask (bit n = MDATA1 monster type n) and an Integer lair monster id (0 = none).
4. Teleporters: a slot-count record (20 on floors 1-9, 10 after), then slots of x, y,
   destination x, y, floor. Destination floor 0 means random (probable). Unused slots
   have x = 0; coordinates are 1-based.
5. Chutes: a slot-count record (10), then slots of x, y, floors dropped.

Square flag bits follow the original map editor's checkboxes: 0 right (east) wall, 1
upper (north) wall, 2-3 right/upper door, 4-5 right/upper secret door, 6-9 face
north/east/south/west, 10 extinguish, 11 pit, 12 stairs up, 13 stairs down, 14
teleporter, 15 water, 16 quicksand, 17 rotator, 18 anti-magic, 19 rock, 20 fog, 21
chute, 22 "stud" (unknown). Walls and doors line up with area edges, which confirms
bits 0-3 and the axis directions. Floor 8 has two teleporter squares with no slot.
