# Balance changes

The remake's departures from the original numbers. The originals are in `docs/formats.md`.
Numbers marked "start" are first guesses, to be tuned in `patches/modern.toml`.

## Original rules (from the game's HelpLesson, MORDOR.HLP)

- **STR**: to-hit and melee damage.
- **INT**: to-hit, max SP, identifying items and monsters.
- **WIS**: to-hit, max SP, recognising chest traps, identifying, slipping out of
  combat unhit.
- **CON**: HP per level (bonus above 16, until the guild's HP level cap), the chance
  a raise succeeds, and resisting poison, disease, stoning, aging, paralysis, breath
  and monster spells.
- **CHA**: monsters joining as companions, how long companions stay, monsters turning
  hostile.
- **DEX**: to-hit, disarming traps, chest-opening time in combat, resisting theft,
  slipping out of combat unhit, surprising monsters.
- Natural stats can be raised up to the race maximum + 5. Max SP and item and guild
  requirements use natural stats only.
- HP is gained only when levelling the guild that is the character's highest level.
- Guild abilities and A/D are not added together: the best guild counts. Nomad
  raises A/D fastest but tops out sooner, at about 75/75 by level 30.
- Spell power follows the spell level of the best guild that teaches the spell, and
  SP cost follows the cheapest. SP refills only in the city.
- A morgue raise often costs 1 CON. Failed raises can cause "complications" that
  drain life and stats, and older characters are more likely to suffer them.
  Waiting for a rescue party ages the character.

## Original guild level cost (measured 2026-11-03)

Read from the guild's "Exp. Info" box in v1.1 under winevdm, after editing the save's
guild level and the MDATA1 XP factors (73 readings, levels 1-250). To go from level L to
L+1 a character needs XP(L) in total, the cumulative amount, not the cost of that level
alone:

    XP(L) ≈ R(race) × (G × L² + 8.256 × L − 4.155)        G = guild XP factor (8-32)

- The fit is within 1 XP at every reading. The L² term scales exactly with G; the
  L term ignores G.
- R grows more slowly than the race's XP factor r: about
  8.756 × r / (r + 0.65) + 0.3185 × r + 2.83.
  - Measured R: Human 11.01, Morloch 11.65, Dwarf 12.17, Ogre 12.63,
    Elf/Giant/Gnome 13.05, Osiri 13.45.
  - Troll is 13.86, from the fit. A Troll pays only 26% more than a Human, though its
    factor is 3x.
- Examples for a Human:
  - Nomad: 133 XP for level 2, 9,672 at level 10, 322,000 at level 60.
  - Healer: 1.27 million at level 60.
- Gold to level ≈ 39.13 × L², whatever the race or guild (39 for level 2, 391,313 at level
  100).

## Decided

### Nomad feeds the other guilds (2026-11-03)

The problem: Nomad is the cheapest guild to level (XP factor 8, against 15-32 for the
rest), needs no stats and gives 5 HP a level up to level 30. So levelling Nomad to 30
first is the best opening. The game's own help even recommends it, and Nomad's fast A/D
gain makes it stronger still.

- Nomad's own HP drops to 3 a level up to level 20 (start), then 1 a level after that.
- Each Nomad level passes a share of Nomad's HP and ability ratings (thieving, fighting,
  perception) to the character's other guilds. The share is 20% (start).
- The share counts at most as many Nomad levels as the character's highest other guild
  level. Levelling Nomad ahead of the other guilds therefore gives nothing extra.
- A character whose only guild is Nomad has its Nomad ratings cut to 70% (start). A
  pure Nomad is the hard mode.

### Melee uses STR and DEX (2026-11-03)

Giants stay the best melee race, but by less. STR max stays 25.

- Melee to-hit and damage bonuses use a melee rating of (2 x STR + DEX) / 3 (weights
  start) in place of STR alone.
- At race maximums Giants rate 22.7, Morloch and Troll 20, Ogre 19. Under STR alone
  the Giant lead is 5 points; under this rating it is 2.7.
- Giant guilds: Nomad, Warrior, Scavenger (replacing Seeker) and Healer. Giants can
  meet both new guilds' requirements (Scavenger DEX 14; Healer INT and WIS 14, against
  their max 17). Healer is Neutral-only, and Giants are always Neutral.

### Magic guilds (2026-11-03)

In the original, Wizard has 46 spells and none of its own: 19 are shared with Healer,
18 with Mage, 21 with Seeker and 15 with Sorcerer. Now each caster has its own school
and one passive. Max SP stays INT + WIS for every guild.

- **Sorcerer, elements** (fire, cold, electric, element, resists, protection): elemental
  spells ignore part of a monster's resistance.
- **Healer, life** (heal, cure, raise, resists, dispel and abolish undead, a few wound
  spells): the party slowly regains HP in the dungeon. Healer is also decent in melee:
  fighting ability 5 to 8 (Villain's) and 3 HP a level up to its cap, from 2 (start).
- **Mage, mind and companions** (charm, control, bind, dominate, mind, visual):
  companions are bound more strongly and stay longer, scaling with CHA.
- **Wizard, death, banishing and travel** (cause wounds and harm lines, kill, banish,
  draining touch, summon shade, location and movement): killing with a Wizard spell
  restores some SP. It gives up Mage's mind and visual spells, and keeps its HP level
  cap of 41.
- Paladin, Villain and Seeker spell lists are unchanged.

### Stat passives (2026-11-03)

One new effect per stat, on top of the originals above. Numbers are start values;
"above 15" counts natural plus modified stats.

- STR: +1 defense per point above 15.
- DEX: +3% critical hit damage per point above 15.
- CON: its bonus HP keeps coming after the guild's HP level cap, at half rate.
- INT: +2% damage from attack spells per point above 15.
- WIS: +2% healing per point above 15, and it takes over resisting monster spells from
  CON. CON keeps breath, poison, disease, stoning, aging and paralysis.
- CHA: shop prices 1% better per point above 10, both buying and selling (selling
  companions too).
- Melee to-hit comes from the STR/DEX melee rating, so INT and WIS no longer add to it.

### Guild XP is cheaper per level (2026-11-03)

Each level's own cost, XP(L) − XP(L−1) from the original formula above, is divided by a
divisor that depends on the level:
- 2.5 up to level 60;
- falling linearly to 1.5 by level 70;
- 1.5 from then on.

Savings carry forward. A Human Warrior needs 256k XP for level 60 (640k in the original),
377k for 70 (870k) and 978k for 100 (1.77M).

Dividing the cumulative totals instead was rejected: around level 70 a single level would
cost about 2x the original. Race and guild scaling stay as in the original. The gold
needed to level is unchanged for now.

### Experience boosts (2026-11-03)

Each boost multiplies the XP a character gets from combat. It lasts until the party
changes floor, going back to town included.

| Boost | Multiplier | Who it affects | Where it comes from |
|---|---|---|---|
| Potion of Experience | 1.5x | the drinker | General Store (high price) and drops |
| Greater Potion of Experience | 2.0x | the drinker | drops only, floor 5 and deeper |
| Charm of Experience | 1.5x | the whole party | Seeker spell, spell level 16 |

- Boosts do not stack: only the strongest active boost applies to a character.
- Charm of Experience sits above Resist Magic (15), the top of the original Seeker list.
- Its SP cost, and the potion's price and drop rates, are start values to tune.

## Taken while building (review at play-test)

Where the original's rule isn't known, these are stand-ins close to its spirit. Each
can be changed in `patches/modern.toml` or here.

- **Combat start values**: hit chance attack / (attack + defense), clamped to 5-95%
  (a ratio, since the original's A/D runs from about 6 on floor 1 to 900 on floor 15).
  Weapon damage is 1 to power × 6 plus (STR − 10) / 2; bare hands count as power 2.
  Monster damage is 1 to level × 2 + attack / 20. A kill is worth
  hp × level × 1.5 + attack / 2 XP. Gold is level × 5-25 per monster, and a cleared
  group leaves an item 15% of the time.
- **Guild A/D**: each level adds 3 × the guild's fighting ability to attack and
  defense, and the best guild counts (Nomad reaches about 71 by level 30; the help
  says 75).
- **Joining guilds**: the first guild after Nomad is free, as in the original; after
  that each costs 5,000 gold × the guilds already joined.
- **Morgue**: rescuers charge 250 × floor², raising 100 + 50 × (total levels)², or
  all your gold if you have less. Raising always works, and loses a point of CON
  half the time.
- **Store stock**: the store-only items (level 0) at their listed prices; it buys at
  half price.
- **Dungeon view**: the original's stone, floor and door textures on a 3D view up to
  four squares deep, instead of its 50 pre-drawn frames, so it scales to any window.
- **Party**: you plus up to three saved characters as allies, as upstream; Good and Evil
  won't travel together. Kill XP is split evenly, then each member gets the catch-up
  bonus for levels behind the party's best; gold is split evenly. Dead members are
  carried and reach the morgue when the party returns. If the character you control
  dies, control passes to the next living member.
- **Party AI**: each character has macros (ordered "when ... do ..." rules, first match
  that can be done wins) with Fighter, Healer, Caster and Thief starting sets, and a
  default decision tree behind them: leave peaceful monsters alone, raise, mass-heal,
  heal, cure, drink a potion when low, group spells on groups, a spell on a foe that
  would take three or more sword rounds, defend when nearly dead, else gang up on the
  weakest foe.
- **Spells**: the guild rates from MDATA1, narrowed to the four caster schools (location
  and Charm of Opening kept for all). Spell power is the best guild's level / rate.
  Damage is base + 0 to per-level x power (INT +2%/point above 15); healing likewise
  (WIS +2%). Kill spells slay 25% + 4% per power over the monster's level, less its
  magic resistance; mind spells put a foe out for 2-4 rounds. Banish and dispel work on
  their monster type only, 60% + 5% per power over its level. Raise Dead brings a member
  back with 1 hit (Resurrect: full) 75% + 2%/power of the time. Sorcerers ignore half a
  monster's resistance with elemental spells; Wizards regain a tenth of their SP for a
  kill with a Wizard spell.
- **Spell points**: (INT + WIS) x (5 + 2.5 x (best guild level - 1)) (the original's
  level 1 gives 5 x (INT + WIS)); 0.5% of max back per step; full in the city.
- **Healer passive**: with a living Healer in the party, everyone regains 1% of max HP
  (at least 1) per step. Healer's fighting ability is Villain's (0.8) and it gets 3 HP
  a level up to its cap.
- **Nomad's share in practice**: other guilds' ability ratings add 20% of Nomad's
  rating (counting Nomad levels up to the best other guild's level), and a level in
  another guild gains 1 extra hit when Nomad is already at that level. Pure Nomads rate
  at 70%.
- **Monster specials**: each special lands on one hit in four, against the matching
  resistance: the race's %, +50 for a resist spell, +3% per CON (WIS for monster spells)
  point above 12. Poison and disease tick per step; paralysis lasts a few rounds;
  stoning holds until Restore Flesh or the morgue. Breath (one round in five) hits
  everyone for level x 2-4. Thieves steal 10-30% of a purse, or a loose item, and flee.
  Monster casters spend one round in three on a spell for casting level x 1-3.
- **Peaceful groups**: the monster's peace chance, +3% per CHA point above 10 of the
  character you control; you can walk on through. Attacking, casting at them or opening
  their chest makes them hostile; allies leave them alone.
- **Chests and boxes**: lairs always have a chest; elsewhere a chest at the monster's
  chest % / 3 and a box at its box % / 2. 30% are untrapped; the rest carry one of the
  help's ten traps. The best perceiver guesses the trap (right 35% + skill); the best
  disarmer tries first: 30% + thieving x 4% + 2% per DEX above 12 - 3% per monster
  level. Chests hold level x 60-240 gold and 1-2 items, boxes level x 20-80 and maybe one.
  Magic locks follow the floor table.
- **Items**: found items are known 2% per INT + WIS point (else "Unidentified Potion"
  and so on); the store identifies for a tenth of the price and takes cursed items off
  you for half. Stat potions and tomes raise a stat by one, to the race max + 5.
  Experience potions cost 7,500 (Greater: drop only, 30,000 value); a found item is an
  experience potion 4% of the time (Greater 30% of those, floor 5+).
- **Charm of Experience**: 60 SP, needs INT and WIS 14.
- **Companions**: up to four. Charms take 35% + 5% per power over the monster's level,
  times the binding strength (Mage members 1.5x; +4% per CHA point above 10). A bind
  lasts about 100 steps x strength; badly hurt companions may wander off. Holding &
  Confinement sells tame monsters of floors 1-4 at 100 x level² x hits, buys at a third,
  and rebinds for a tenth.
- **City**: the bank pays fees when the purse runs short (as the original). The Seer
  charges 200 to locate a party member or a monster's lair. Every fifth level a guild
  may set a quest (half the time): slay a monster from floor level/5 before the next
  level. Rescuers fail 3% per floor and keep the fee; raises fail 5% plus 1% per year
  past half the race's lifespan, aging the body a year. Resting in the city ages you a day.
- **Squares**: water may hurt members wearing 12+ defense of gear (a 1 in 4 chance per
  step); quicksand hurts and may swallow a loose item; extinguish squares strip spell
  effects; anti-magic blocks casting; fog hides the view. Random teleporters keep the
  party together.
