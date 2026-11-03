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
