# The Dungeon

The dungeon is the heart of the game: a maze of 30×30 floors full of monsters,
chests, traps, and treasure. This page covers moving around, fighting, looting,
and getting back out alive.

[Back to Help index](index.md)

## How to play

### Moving and mapping

- You explore one step at a time, turning and walking through the maze from a
  top-down view.
- Your **map is always visible while you play** and fills in as you explore. Map
  knowledge is shared across your characters — once a floor is mapped, you don't
  have to map it again.
- Watch for **stairs** and **chutes** to move between floors. The action to use a
  staircase or drop down a chute is labeled **Climb** (this is the action the
  original game called "Take").
- Some squares teleport you or spin you around and leave you **lost** — your
  position, floor, or facing may be unreliable until you step back into known
  territory or otherwise get your bearings.

### Your party on screen

- Click or tap the whole **name/stats card** to open the currently controlled
  character's sheet, or use **RB / R1** on that player's gamepad. The card shows
  **XP / HP / SP**; each co-op card opens its own character.
- In solo play, character allies sit at the **top right** and monster companions
  at the **bottom left**. The desktop map uses the available upper-left space;
  Mobile extends the map behind the unchanged direction circles.
- Saved-character allies show a **green up arrow** when they have enough XP for
  the next active-guild level, or a **red up arrow** when pinned at that guild's
  quest cap. Red takes precedence. Green is not a promise that a quest or fee
  will not block training. Generated Tavern allies do not show these arrows.
- An ally's menu provides **Inspect / AI behavior** and, for eligible real
  saved members, **Switch to this character**. Switching transfers control,
  not ownership or an extra turn. Inspection covers Equipment, AI behavior,
  Spells, and Stats; see the [Character Sheet](character-sheet.md#ally-ai-and-inspection).
- Companion faces summarize current **binding/contentment stability**, separate
  from HP color. A lock means permanent binding. Open the companion's menu for
  details or learned Control/Restrain/Domination targeting.

### Combat

Fights happen in real-time rounds, driven by a combat timer that ticks about once
per second. Each round your hero and character allies act once, with monster
companions taking their own sub-turns. Your hero's options:

- **Attack** — swing your equipped weapon(s) at a monster group. You get a base
  number of swings from your active guild plus extra swings from your weapons; if a
  target dies, remaining swings move on to the next monster. Some monsters may be
  resistant to attacks if your weapon is too low level.
- **Defend** — give up your swing for the round in exchange for much better
  defense.
- **Tank** — draw the monsters' attention to yourself while still attacking (at
  reduced damage). Useful for protecting weaker allies. Select it by pressing the
  **Defend** button twice: the first press queues Defend, and pressing again
  switches to Tank.
- **Cast a spell** — offensive spells chain through a monster group; healing and
  buffs support your side. (See the [Character Sheet](character-sheet.md) for
  casting.)
- **Use an item** — potions, scrolls, charged items, and more.
- **Move away** — walking away from an active fight interrupts the encounter.

Normal rounds resolve in **fixed participant order**, with allied character
actions and companion sub-turns before the monsters. There is no DEX initiative
roll. The exception is a friendly encounter becoming hostile through its
aggression check: the monsters receive a **monster-only opening round**.

A manually chosen Attack/Defend/Tank stance **sticks within the current
encounter** until replaced. Automatic defaults, including a new encounter or a
return from a one-shot action, instead use the controlled character's
capabilities: **Attack** if enabled, otherwise **Tank** if enabled, otherwise
**Defend**. These flags do not forbid you from manually choosing Attack or Tank.

You can select a monster group during a friendly encounter without provoking
it. Offensive spells and spell-bearing items use that selected group when
executed. Support items open their spell's appropriate recipient or destination
picker; canceling does not consume a charge.

### Chests and traps

After a victory you may find a **chest** or **box** (chests hold the better loot).
Before grabbing the contents:

1. **Inspect** for a trap. Chests can be locked and trapped, and spotting the trap
   is based on WIS, with item WIS helping.
2. **Disarm** the trap if you can — success depends on your thieving ability plus
   WIS and DEX, and gets harder the deeper you are.
3. **Open** to claim the loot. If you misjudge it, the trap goes off — fire, shock,
   poison, disease, and worse.

Gold is shared among that encounter's participating real characters, with
generated allies' shares treated as expenses. Chest items collected by an ally
go to that player's currently controlled real character. If the pack is full,
uncollected items stay in the opened chest for later collection rather than
being silently discarded.

### Staying alive

- Keep an eye on your HP and spell points. When either runs low, head back to the
  stairs and return to town to heal, bank your loot, and restock.
- The deeper you go, the better the rewards — and the deadlier the monsters and
  traps.
- Downed saved-character allies can be revived before returning. If brought
  back unrevived, they pay their own normal death/recovery costs; generated
  adventurers use their separate town revival rules.
- Floor changes and entering/leaving town are save checkpoints, but ordinary
  room moves are not. Exit normally to flush recent progress; see
  [saving and checkpoints](settings.md#saving-and-checkpoints).

## Differences from the original

- **Top-down, not first-person.** The original was a 3D first-person corridor
  crawler. This remake presents the dungeon as a 2D top-down grid with fog-of-war
  and paperdoll monster art.
- **Death sends you back to town.** Instead of leaving a corpse to recover via a
  morgue, falling in the dungeon respawns you in town. An ordinary death is
  forgiving (you're always raised), though some penalties apply. Walking or
  teleporting **into solid rock** is the dangerous exception and can permanently
  cost you stats — so use the teleport tools carefully.
- **Banked gold is safe.** Gold found during a dive is carried until you return
  to town, where it is auto-deposited. Certain monsters can steal carried gold;
  you reclaim it if you win the fight, but lose it if the thief escapes. No trap
  steals gold.
- **Tank** is a new stance unique to this remake: unlike a passive guard, a tank
  keeps attacking (for reduced damage) while drawing the most aggro to protect the
  group.

[Back to Help index](index.md)
