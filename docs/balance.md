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

## Original rules (read from MORDOR.EXE, 2026-11-04)

How these were read: docs/formats.md, "Reading MORDOR.EXE". The remake uses them as below.
Each place it departs from them is marked "remake", and the original's bugs that it fixes
are listed at the end. Notation:
- L is a guild level and r that guild's ability rate from the table (Warrior fighting 1.2).
- d100 is a roll of 1-100.
- "w" fields are table words (docs/formats.md).

**Melee** (`combat.rs`):
- **One swing:**
  - hit% = 50 + 50 (att − def) / att when att > def, else 50 − 50 (def − att) / def, clamped to 2-98.
  - Damage = (ln(L + 5) + 1) / ln 1.2 × s / 30 × a roll of 0.6-1.6 around (s + 10) / 20 × mult. Here s is STR, compressed: s − ln s (ln s − 2.4) 2.25 + 1.
  - The attack/defense gap takes up to (def − att) / 8 off, or adds it when the attacker is stronger.
  - mult above 1 is compressed to mult − ln(mult)².
- **Swings:** a character swings its swings plus the weapon's (w9). A miss, or a hit that doesn't kill, ends the turn; a kill moves on to the next foe. Every character carries item 0, Hands (two swings at half damage), in its first slot; it is the weapon whenever no other is wielded, and can't be sold, dropped, stolen or destroyed.
- **Guild swings:** the first extra one comes at level ln(11 − r) × 50, and each next at ln(11 − r) × 2 × the last. A Warrior gets them at 91 and 324.
- **Multiplier:**
  - Fighting: 0.6 + (ln(100 + L) / 1.75 − 2.3) × r² / 2, times the weapon's w18.
  - A critical hit adds 5 and a backstab 2. Their chances are (ln(L/999 + 1) × 100 × ln(r + 1) / 3 − ln(monster level / 999 + 1) × 100) / 2, capped at 50%. Backstab divides by 2 instead of 3 and caps at 75%. Weapons with the critical or backstab flag add more.
  - Size: ±0.1 per step of race size against monster size.
  - An invisible monster halves the multiplier unless you see invisible.
  - Weapon-resistant monsters ignore weapons below half their level, and weapon-immune monsters ignore all weapons.
- **Attack and defense:**
  - 10 each, plus the stat bonus, plus the best guild's A/D, plus items.
  - The stat bonus is +2 per point of STR and DEX above 12, +1 per point of INT and WIS above 14, with mirror penalties below 8. In the remake it is +4 per point of the melee rating above 12 (or below 8).
  - Guild A/D: each guild adds its w114/w116 every Int(w119) levels up to its HP cap (w1). After that the gap is CInt((w1 / 2 + L / 18) / (w119, + 3 for Nomad)) levels, up to level w118. Nomad reaches 45/45 by level 30.
- **Monsters:**
  - Each group attacks one member a round, chosen 100/50/25/12/6 by party position, with up to five of its monsters. Each attacker hits the member, or one of its companions.
  - Monster swings use its attack (w0), level (w37), STR (w38) and multiplier (w46). Critical adds 2 and backstab 1.
  - Breath, acid and electrocution are N-based, with N ≈ (ln X + X/15) / ln 1.08 − target level / 20, and resistance takes half off. They come 50% of the time, or 80% (acid and shocks 90%) against targets above 1.25 × the monster's level, and 20% (10%) below 0.75 ×.
  - Monster spells come from its classes (w24), at a level near its own. Damage = base × (1 + ln(l + 1) / ln 5) + l / 10 × per level, at caster level w37 / 4.
- **Specials after a hit that hurt:**
  - Poison, disease and paralysis go past d100 against resistance; paralysis adds CON.
  - Aging and draining go past d100 against CON × 2.5 − 2 × level.
  - Stoning: CON × ln CON / ln 2.5 − level saves.
  - Item destruction and theft need no hit. DEX protects items; thieving and wounds protect purses.
- **XP** comes per point of damage dealt, to the dealer:
  - d (0.9 − ln(d + 1)/10)(5 + level/10)(2 − ln d/10) / (1 + 3 L/999).
  - Less 4% per other party member.
  - Companions earn their master a quarter.
  - The remake's depth and catch-up bonuses apply on top.
- **Peace:** a good group meets a good leader in peace, and a neutral group 40% of the time. Each round a peaceful group turns hostile at a chance set by the two alignments (15-100) minus 7.5 ln CHA.
- **Leaving** a fight gives the monsters a free attack, 50% + 5% per point of their DEX over the leader's (5% at least).
- **Ailments** (timAilment_Timer; one tick is a combat round in a fight and a step outside, never while idle):
  - Poison and disease wear off when d200 + 2 (CON − 10) − 2 × strength beats 185 (195 for disease). Until then poison costs 2 hits a tick and disease 1. An ailment already there isn't renewed.
  - Paralysis has the monster's power level p as its strength. Each tick it holds at clamp(ln(0.6 p) / ln 1.02 − (((ln L + L/10) / ln 1.05) / 10 + 1.5 CON + paralysis resistance), 5, 90)%, L the current guild level, and it ends at once when no monsters are left. Meanwhile the stat bonus to attack and defense is lost.
- **Level-up HP:** the guild's hits; two times in three, d(hits / 5) more; and d(CON − 16) for CON above 16.
- **Quests:** from level 5, at the guild's quest chance (w32).

**Treasure** (`treasure.rs`):
- **Containers:**
  - One d99 roll for a chest (w29) or a box (w28), +25 in the monster's lair.
  - Chests are trapped 89% of the time and boxes 74%, with a trap drawn from the monster's trap list (w35).
  - Magic locks: remake. Rare on floors 1-5 (the floor table); the original uses w6%.
- **Gold:**
  - For gold exponent e, B = e × 10^(e − 0.5) / 4.
  - Each group carries a quarter to five quarters of B × log2(level + 1), 500 at least.
  - The total is × ln(n + 1)² / 2 × n / 2 for n monsters, and × 1.5 if magically locked.
  - It is paid when the fight ends, or from the container (a chest pays half again).
- **Thieving:**
  - WIS / 4 + DEX / 2, plus (4.5 + (L + 1)^(1/ln 15)) × r × 1.4 for the best guild.
  - Less 9.375 × ln(monster level) × ln(floor + 1).
  - Clamped to 5-98%.
- **Opening:**
  - Opening disarms at the opener's thieving %. A slip sets the trap off; disarming gives XP.
  - In a fight, opening takes CInt(10 − 2 ln(thieving − 10)) rounds (2 at least). While the opener works, monsters hit it harder.
- **Trap guess:** WIS less ln(level) × ln(DEX) × 2.5 of the monster, plus thieving / 1.2 and ln(thief level)², against d100. A failed guess names a random trap; the confidence bar shows 1-4 bullets.
- **Trap effects** are scaled by the floor L:
  - Exploding: 1 to 4L ln(L+2) to everyone.
  - Fire: 1 to 6L ln(L+2) to everyone, less fire resistance / 2.
  - Poison and disease: everyone.
  - Slime: eats a random item of the opener's (50% + 2L).
  - Fate: re-rolls the opener's hits and SP.
  - Blackout: strips the opener's spells.
  - Fear: scares off the opener's companions that aren't fully bound.
  - Withering: ages the opener 28 days.
  - Teleporter: moves the party (remake; the original moves only the opener).
- **Items:**
  - d100 (−25 for a chest) against the monster's item % (w66), then a tenth of it for each further item.
  - Each item comes from one of the monster's ten classes (w51-60), at a level from half its item level (w65) to all of it, weighted by rarity (w6).
  - The finder knows it when the item's difficulty, 25 + (ln(mult + 1) + 0.3) ln(w17 + 1) × level, is within (INT + WIS)/2 × ln((best level + 5)/10 + 1), and only for items up to level 5.

**City** (`store.rs`, `game.rs`, `monsters.rs`):
- **Stock:**
  - The store's and Confinement's stock is shared by every character (saves/store.json). Store-only (level 0) items never run out.
  - Buying costs 1.5 P / (ln s + 1.15) with s in stock (1.3 P for the last one, 0.45 P with nine); store-only items cost P.
  - The store pays 0.9 P / (s + 1.15), a third of that unidentified, less for used charges, and nothing for known cursed items. Items sold go back on the shelf.
  - Resting in the city ages a day per 3.25% of hits missing, and restocks: while d100 ≤ days (up to 50), one more event. Each event is 60% new stock from the floors reached and 40% sold off.
- **Item services:**
  - Identify: 0.23 P / ((s/2 + 1) × 0.9) per step (two steps).
  - Uncurse: 1.6 P, and the item comes off.
  - CHA's passive (remake) takes 1% a point above 10 off every buy and sell price.
- **Guilds:**
  - Joining: free while you belong only to your current guild; otherwise A² × 5356. A sums (rate + 1) × 1.25 over the guild's abilities and max spell level / 3 over its classes: Nomad 1.7 million, Ninja 10.9 million.
  - Level gold: see "Original guild level cost" above.
- **Morgue:**
  - Raise: 2,912.32 × max hits / 1.5, refused without the gold, except free when the best guild level is 5 or less. It works when Int(Rnd × 100) ≤ clamp(CInt(35 CON / 4.5 + 10), 10, 98).
  - A failed raise brings complications at age / (lifespan / 2): −5 to every stat (not below the race minimum), 15 years, max hits / 1.2, and the raise then goes through. Without them the body ages 54 days and stays dead.
  - A raise ages 54 days and loses a point of CON (50 − 2 CON)% of the time.
  - Rescue from floor F: 372,567.52 ln(F + 1) F. The rescuers are lost 4% of the time per floor.
- **Seer:** a party member costs 132,567.52 ln(F + 1) F; a monster's lair costs half that at its level, up to level 12.
- **Confinement:**
  - A companion's value is (500 att + 500 def + 250 hits) × ln(STR + DEX) × ln(level + 1), plus its specials, spells and resistances.
  - It sells for 1.5 × value / (ln s + 1.15) and is bought back at 0.9 × value / (s + 1.15).
  - Bound companions start at binding level 5 + level (10 at most).
  - The bindsmen add 4 levels (12 at most) for power level × 1237 × new level × old level / 4.

**Original bugs the remake fixes:**
1. The player's hit roll passes defense 0 (players hit 97%); the monster's defense is used.
2. Defend's ×1.5 defense was computed and then thrown away; it counts.
3. Poison and disease traps caught only resistant characters. Now resistance / 2 is the chance to shrug them off.
4. After a trap, the death check looked at party slot 0 but killed the opener; it checks the opener.
5. Disarm XP used party slot 0's level; it uses the opener's.
6. Draining never lowered STR; it can.
7. Item destruction counted DEX twice. Theft used the controlled character's thieving instead of the victim's.
8. An item's 90% resistance replaced a higher race value; the higher counts.
9. Disease recovery read the poison strength.
10. Gold used ln((level + 1) / ln 2), a misplaced bracket for log2(level + 1).
11. Every charged item bought got a discount (an uninitialised index); prices no longer do.
12. Restocking only ever sold off unaligned stock, and Confinement could pass 9 of a monster.
13. The last binding step was cheap; the cost uses the new level.
14. Opening in a fight with thieving 10 or less raised a run-time error; it takes 10 rounds.
15. The rescue fee used the floor the player typed; it uses the real floor.

## Original guild level cost (measured 2026-11-03, read from MORDOR.EXE 2026-11-04)

To go from level L to L+1 a character needs XP(L) in total, the cumulative amount, not
the cost of that level alone. MORDOR.EXE computes it as follows (`original_xp`):

    XP(L) = (500 × 2LG / 12 × L + 344 × (2L − 1) − 2) / 10 × (ln r / ln(r + 1) + 0.3 + r / 25)
          ≈ 8.333 × (G × L² + 8.256 × L − 4.152) × race term

- G is the guild XP factor (8-32). r is the race's XP factor + 1.
- Every step is rounded to 4 decimals (VB Currency). The total is rounded half to even.
- It reproduces all 73 readings taken under winevdm exactly; the earlier fit was within 1 XP.
- The race term × 8.333: Human 11.01, Morloch 11.65, Dwarf 12.17, Ogre 12.63, Elf, Giant
  and Gnome 13.05, Osiri 13.45, Troll 13.86. A Troll pays only 26% more than a Human,
  though its factor is 3x.
- Examples for a Human:
  - Nomad: 133 XP for level 2, 9,672 at level 10, 322,000 at level 60.
  - Healer: 1.27 million at level 60.

Gold to make a level (`Tuning::level_gold`):
- The sum over l = 1..L of round(30.1 × (2l − 1)), about 30.1 × L².
- Then × (1 + guild w111 / 10). By guild, w111 is: Warrior 1; Paladin 2; Nomad, Seeker,
  Mage and Sorcerer 3; Wizard and Healer 4; Ninja and Villain 5; Scavenger 7; Thief 9.
- So Nomad pays 39 for level 2 and 391,313 at level 100, and a Thief 57.19 × L². (The
  measured "39.13 × L² for every guild" came from Nomad only.)

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

### Match the original's mechanics (audit, 2026-11-14)

An audit compared the rules with MORDOR.EXE and found mechanics the remake had changed
without saying so. The rule now: match the original, except where this document records
a deliberate change. Each of these is to be ported; until it is, the remake's current
behaviour is a known gap, not a choice.

- **Encounters:** match the original. Up to four linked groups (followers by the
  monster's link fields), spawn weights over the floor window, a 1% wanderer, the
  floor-1 starter rule, half to all of the group size, lairs giving their monster 75% of
  the time with followers. Rooms keep their monsters, wounded, until killed and refill
  over time; monsters get a free attack on entering their room, and leaving is capped at
  98%. The remake's 60% chance per room is dropped.
- **Spell learning:** match the original. A guild teaches a spell up to its class cap,
  when CInt(level x rate) <= Int(L/2 + 0.5), and spell power is Int(L/2 + 0.5).
- **Casting in combat:** match the original. Heals, cures, raises and buffs only with no
  hostile monsters present (Cure Paralysis and Teleport work in both); combat spells only
  with monsters present; spells above level 7 not against peaceful monsters; no casting in
  the city. The Party AI heals between fights.
- **Spell damage, healing and SP cost:** the original's formulas (logarithmic damage and
  healing, SP cost falling with level, the group loop, kill spells killing only when the
  roll reaches the foe's hits, flag-4 monsters immune to player spells).
  **Deliberate change:** the remake's growing max SP pool and its gradual SP regen stay,
  instead of the original's flat 5 x (INT + WIS).
- **Items:** match the original. Equip limits (two hands, the item class's count, the
  guild and guild-level rules, natural stat requirements, no stat below the race minimum);
  worn stat modifiers on a separate modified-stat array; levitate, invisible, protection
  and see invisible from worn items; wearable spell items stay when their charges run out
  (only item types 23-28, 30 and 31 are used up); found items with a random alignment,
  and the store's Realign; store stock kept per alignment; the four identification
  levels; cursed items can't be sold.
- **Morgue (ported):** match the original. No raise or rescue without the gold (free for a
  character whose best guild level is 5 or below); the original's raise odds,
  complications and CON loss; a member raised in town gets the window's +10.
- **Stand-ins replaced by the original rules:** paralysis wear-off (ported, and paralysis ends
  with the fight), poison and disease ticking in fights, guild quests, pits, chutes, water
  and quicksand, companion join offers, charm and binding, death from old age.
- **Theft and monster spells:** match the original, including theft's gold rule and
  monster spells hitting the whole party.
- **Real-time timers:** where the original runs a rule on a wall-clock timer (ailments,
  water and quicksand, binding decay, old age, room refills), one tick is one combat round
  in a fight and one step outside. **Deliberate change:** idling never advances them.
- **Everything else the audit listed** (teleport and displacement, buffs, item cast
  levels, bank item storage, Pool, restocking and the minor items): match the original by
  default. Where a rule would hurt play on a phone, it comes back to the owner as its own
  decision first.

## Taken while building (review at play-test)

Where the original's rule isn't known, these are stand-ins close to its spirit. Each
can be changed in `patches/modern.toml` or here.

- **Dungeon view**: the original's stone, floor and door textures on a 3D view up to
  four squares deep, instead of its 50 pre-drawn frames, so it scales to any window.
- **Party**: you plus up to three saved characters as allies, as upstream; Good and Evil
  won't travel together. Kill XP is split evenly, then each member gets the catch-up
  bonus for levels behind the party's best; gold is split evenly. Dead members are
  carried and reach the morgue when the party returns. If the character you control
  dies, control passes to the next living member.
- **Party AI**: each character has macros (ordered "when ... do ..." rules, first match
  that can be done wins) with Fighter, Healer, Caster and Thief starting sets that name
  the character's best known spell of each kind (unedited sets take up new spells at
  each level), and a
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
- **Items**: unidentified finds show as "Unidentified Potion" and so on. Stat potions and tomes raise a stat by one, to the race max + 5.
  Experience potions cost 7,500 (Greater: drop only, 30,000 value); a found item is an
  experience potion 4% of the time (Greater 30% of those, floor 5+).
- **Charm of Experience**: 60 SP, needs INT and WIS 14.
- **Companions**: up to four. Charms take 35% + 5% per power over the monster's level,
  times the binding strength (Mage members 1.5x; +4% per CHA point above 10). A bind
  lasts 30 steps x binding level x strength (the original's leaving rule isn't read
  yet); badly hurt companions may wander off.
- **City**: the bank pays fees when the purse runs short (as the original). A guild's
  quest asks for a monster from floor level/5 (the original's choice isn't read yet).
- The Teleporter trap moves the whole party, and the container is lost.
- **Squares**: water may hurt members wearing 12+ defense of gear (a 1 in 4 chance per
  step); quicksand hurts and may swallow a loose item; extinguish squares strip spell
  effects; anti-magic blocks casting; fog hides the view. Random teleporters keep the
  party together.
