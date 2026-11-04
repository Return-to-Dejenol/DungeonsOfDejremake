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
  unknown; w28 size (1 small, 2 normal, 3 big, 4 very big); w29 unknown; w30 lifespan;
  w31-32 f32 XP factor.
- **Guild**: w0 average HP per level up to w1; w1 that level cap; w2 HP per level after it;
  w3-4 f32 XP factor; w5 unknown (Nomad 90, others 1); w6-11 stat requirements; w13
  alignment mask; w15-26 six f32 abilities, each x10 as the refcard shows (thieving,
  backstab, critical hit, multiple swings, fighting, perception); w32 quest chance %;
  w33-68 eighteen f32 spell rates, one per spell class: a spell is taught at guild level
  `spell level x rate` (Paladin heal rate 3: Minor Heal at 6, Heal at 12, as the help's
  lists); w69-108 eighteen more f32, unknown; w109 race mask (bit n = race n); w111-118
  unknown.
- **Item**: w0 id; w1-2 attack/defense (negative on cursed items); w3-4 u32 price; w5
  level found (0 = sold in the store only); w6 probably rarity; w9 weapon power (the
  bracketed number in item info); w11 the 1-based number of the spell it casts
  (potions, scrolls, rods); w13 charges (potions 3, scrolls 4, rods 25); w15 guild-usable
  bitmask (bit n = guild n; 4095 = all); w18-19 f32 damage multiplier; w23 index into
  item types; w26-31 stats required (probable).
- **Monster**: w0 attack; w1 defense; w2 id (0-400); w3 hit points; w5 portrait (1-based
  tile of MG11's `MONS` sheet, 60x60, ten to a row); w7 level found (1-16); w8-19
  resistances (race order); w20 casting level and w24 the spell classes it casts (mages,
  priests); w22 special attacks (bits: 1 poison, 2 disease, 4 paralysis, 8 fire breath,
  16 cold breath, 32 acid, 64 electric breath, 128 drain, 256 stoning, 512 aging
  (probable), 1024 critical hits, 2048 steal gold, 4096 destroy items, 8192 steal
  items; checked against the help's examples); w26 % chance a group is peaceful
  (Clean-Ups 66, "usually docile"); w27-29 % box, chest and item chances (probable);
  w32 group size; w45 monster type (rooms spawn by it).
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
the city buildings (120x87, in the order Dungeon, Guilds, Seer, Morgue, Bank, Store,
Confinement), the Exit sign, the main menu panel (600x400) and two portraits, as VB3
form pictures; the importer writes only these out.

## Characters (MDATA4)

Created at run time; MDATA9 lists the names. Record size not worked out yet. Offsets are
from the start of the name, a fixed 30-byte space-padded string. Confirmed by editing a
save and reading the result in the game:
- +30 race (0 = Human)
- +36 age in days, as an f32
- +56-66 the six stats
- +1646 XP, as a VB Currency (i64 × 10,000)
- +1654 gold, also a Currency
- +1662 current HP, then max HP
- +1678 Nomad level

Other guild levels probably follow +1678; unverified.

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
