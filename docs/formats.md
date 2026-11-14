# Original data formats (MDATA*.MDR)

Worked out from the shareware v1.1 files; the parser is `crates/rules/src/mdr.rs`. Race and
guild fields were checked against the fan PlayGuide refcard
(https://www.mocagh.org/miscgame/dejenol-refcard.pdf).
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
  unused; w14-25 resistance % (fire, cold, electric, mind, disease, poison, magic, stone,
  paralysis, drain, acid, unused); w26 alignment mask (1 good, 2 neutral, 4 evil); w27
  unknown; w28 size (1 small, 2 normal, 3 big, 4 very big; against monster w64); w29 unknown; w30 lifespan;
  w31-32 f32 XP factor.
- **Guild**: w0 average HP per level up to w1; w1 that level cap; w2 HP per level after it;
  w3-4 f32 XP factor; w5 unknown (Nomad 90, others 1); w6-11 stat requirements; w13
  alignment mask; w15-26 six f32 ability rates as the refcard shows them (thieving,
  backstab, critical hit, multiple swings, fighting, perception; Warrior fighting 1.2);
  w32 quest chance %; w33-68 eighteen f32 spell rates, one per spell class: a spell is
  known when CInt(spell level x rate) <= Int(guild level / 2 + 0.5), the caster's spell
  level, the number the help's lists show (Paladin heal rate 3: Minor Heal at spell level
  6, guild level 11); w71+2c f32 the max spell level of class c, above which the guild
  never teaches it; w109 race mask
  (bit n = race n); w111 gold factor (tenths added to the level fee); w114 and w116 f32
  attack and defense gained per A/D step; w118 the last level that gains A/D; w119 f32
  levels between A/D steps.
- **Item**: w0 id; w1-2 attack/defense; w3-4 u32 price; w5 level found (0 = sold in the
  store only); w6 rarity (the weight among items of its type and level); w7-8 flags
  (1 levitate, 2 invisible, 4 protect, 8 see invisible, 16 better criticals, 32 better
  backstabs); w9 swings it adds (the bracketed number in item info); w10 500-511 a guild
  item; w11 the 1-based number of the spell it casts (potions, scrolls, rods); w13 full
  charges (potions 3, scrolls 4, rods 25); w15 guild-usable bitmask (bit n = guild n;
  4095 = all); w17 used in its identification difficulty; w18-19 f32 damage multiplier;
  w20-21 alignment mask (1 unaligned only); w23 index into item types; w24-25 resistance
  mask (each bit gives 90% while worn); w26-31 stats required (probable); w33-38 stat
  modifiers while worn; w40 cursed (2 = sticks on by itself).
- **Monster**: w0 attack; w1 defense; w2 id (0-400); w3 hit points (each new one ± a
  quarter); w5 portrait (1-based tile of MG11's `MONS` sheet, 60x60, ten to a row); w6
  magic-lock %; w7 level found (1-16); w8-19 resistances (race order); w20-21 flags (1
  sees invisible, 2 invisible, 4 resists all magic, 8 resists charms, 16 resists weapons
  below half its level, 32 immune to weapons); w22-23 special attacks (1 poison, 2
  disease, 4 paralyze, 8 fire breath, 16 cold breath, 32 acid, 64 electrocute, 128
  drain, 256 stone, 512 age, 1024 critical hit, 2048 backstab, 4096 destroy item, 8192
  steal); w24-25 the spell classes it casts; w26 its spawn weight within its type
  (Clean-Ups 66); w27 unused; w28 box % and w29 chest % (one roll, the chest wins); w31
  alignment (0 good, 1 neutral, 2 evil); w32 group size; w33 gold exponent; w35-36 trap
  mask (bit n = trap n, bit 0 = not trapped); w37 its power level (damage, traps,
  companion prices); w38-44 stats (STR..DEX, then a spare); w45 monster type (rooms
  spawn by it); w46-47 f32 damage multiplier; w51-60 the item classes its treasure comes
  from (-1 any, below -1 one item, id = -c - 2); w64 size (against race w28); w65 its
  highest item level; w66 item % in its containers.
- **Spell**: w0 id; w1 class (the help's 18: fire, cold, electric, mind, damage,
  element, kill, charm, bind, heal, movement, banish, dispel, resistant, visual,
  magical, location, protection); w2 spell level; w3 SP cost; w6 monsters and w7 groups
  affected; w8-9 damage or healing base and per-level (the help's "15/5"); w10 effect
  (100 mind or death effect, 75-86 the monster type a charm, banish or dispel works on
  (type = code - 75), 200+ a mass spell, 50-60 a resistance in race order, other values
  cures, travel and location effects); w11-16 stats required (Ethereal Portal 14/18/18/
  -/-/18, as the help); w18 resistance slot that weakens it.

## Pictures (MG11.DLL and MORDOR.EXE)

MG11.DLL keeps 8-bit BMP files as resources of type "MORDOR": `MAZE` (50 pre-drawn
50x100 corridor views), `MONS` (the portraits, RLE8-compressed), `DOOR`, `WATR`, `QSND`,
`FOG`, `MISC` (combat icons), `PICS` (toolbar icons), `BLNK` and `ASMO`. MORDOR.EXE holds
its windows' pictures (the city buildings, the menu panel, toolbar icons, arrow buttons)
inside its VB3 forms.

## Windows (MORDOR.EXE's VB3 forms)

RC_DATA 1 is the project directory, listing each form's stream id; RC_DATA <id> is the
form (magic FF CC 2C: the form record, then tagged control records), RC_DATA <id + 1>
its control names. A control record is <len:4 (bit 31: an array member)><id:1>
[<index:2>]<name><type: a built-in id, or FF and a VBX class name><properties>; each
property is <id:1><value>, sized by the class's property table (`vb3props.txt`), with
Left/Top/Width/Height together in twips (15 a pixel). Tags between records: 1 opens a
container's children, 2 closes them, 3 is the next sibling, 4 ends the form. Pictures
are whole BMP (or ICO) files. The importer writes the forms to `FORMS.JSON` and their
BMPs to `<form>_<n>.BMP`; see [ui.md](ui.md).

## The original's text (MORDOR.EXE's string constants)

The p-code keeps each string constant inline as <link:2><length:2><text>. The importer
writes every printable run after a 16-bit length (1-1500 bytes, Windows-1252) to
`STRINGS.TXT`, one a line: all of the program's constants and some harmless noise. The
remake shows the original's words from there, naming each by its id, the 32-bit FNV-1a
hash of its UTF-8 text (`ddj_extract::text::id`), so its source holds the ids, never the
text. A sentence the original built from pieces is built from the same pieces, with the
remake's numbers and names between them. To find an id, hash the exact piece, spaces
included: `python -I scripts/re/textid.py "Not enough gold!"` (or `--find gold` to
search the imported strings). The shareware MORDOR.EXE holds every constant but the registered version's
ordering notices, and data imported before 2026-11-09 takes its text from the bundled
shareware.

## Characters (MDATA4)

Created at run time; MDATA9 lists the names. VB3 random-access records of 2900 bytes:
record 0 holds the version, record 1 the character count, then one character per record
(the last can be cut short). The layout comes from the type descriptors in MORDOR.EXE's
global data (see [Reading MORDOR.EXE](#reading-mordorexe)), so every offset and type is
exact; meanings marked ? are guesses. `ddj-save` (crates/rules/src/mdata4.rs) shows and
edits these fields.

- +0 name (30 chars, space padded); +30 race; +32 alignment?; +34 sex?; +36 age in days (f32)
- +48 attack, +52 defense (both f32, kept up to date as items, levels and stats change)
- +56-68 seven stats (STR INT WIS CON CHA DEX, one spare); +70-82 seven stat modifiers
- +84 carried items: 41 entries of nine Integers (@0 the item); +822 a second list of 41
- +1646 XP and +1654 gold (VB Currency, i64 / 10,000); +1662 HP, +1664 max HP
- +1674 swings per round
- +1678 sixteen guild entries of 24 bytes: @0 level, @2 XP (Currency), three Integers, two f32
- +2062 five companions of 33 bytes: a 15-character name, then nine Integers
- +2347 eight Integers, probably ailments; +2841 the two equipped weapons (carried-item
  slots, -1 for none)
- The record ends at +2883; the other fields are named by offset in `ddj-save`.

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

## Reading MORDOR.EXE

MORDOR.EXE is VB3 p-code. Its procedure and variable names were not kept, but its strings,
constants and record layouts were:
- Each record field reference in the p-code points at an 8-byte descriptor in the VB3
  global data: name hash, next descriptor, type (1 Integer, 2 Long, 3 Single, 4 Double,
  5 Currency, 7 String, 8 fixed String, larger values point at a nested type), and byte
  offset. Array fields have 24-byte descriptors with the element count after the offset.
- Module-level constants are kept as initialised module data.
- Error handlers name their own procedures ("occurred in MONSMOD:monGetGold").
`scripts/re/` rebuilds readable pseudo-source from a local copy of the game with
ghidra-vb3; its output is never committed.

