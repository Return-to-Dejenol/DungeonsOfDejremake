# Settings & Controls

The Settings screen lets you tune audio, choose controls and gameplay options,
manage imported original assets, and configure optional cloud saves.

[Back to Help index](index.md)

## How to play

You can adjust:

- **Audio.** Master, music, and sound-effect volumes, plus a low-HP warning sound
  that alerts you when your health gets dangerously low in combat.
- **Movement mode.** How your directional input (arrow keys, d-pad, or the map's
  on-screen direction zones) maps to moving and turning in the dungeon. There are
  two modes:
  - **Relative** — controls are
    relative to the way you're facing: **up** moves forward, **down** turns you
    around 180°, and **left** / **right** rotate you 90°. This matches how the
    original game moved.
  - **Absolute** *(default for new settings on every platform)* — each direction is a
    fixed **compass heading** on the north-up map: press the heading you're already
    facing to move forward, the opposite heading to turn around, and either of the
    other two to rotate to face that way. An existing saved Relative preference
    is preserved.
- **Touch mode.** Decides when touch-sized (larger) controls are used. Four
  settings:
  - **Auto** *(default when no touchscreen is detected)* — uses touch-sized
    controls only when a touchscreen is present, otherwise compact (non-touch)
    sizing. A touchscreen laptop or 2-in-1 therefore gets touch-sized controls.
  - **On** — always use the larger touch-sized targets.
  - **Off** — never use touch-sized targets (compact sizing everywhere).
  - **Mobile** *(default when a touchscreen is detected)* — touch sizing **plus**
    the phone layout: left-aligned action and hotbar bars and a square on-screen
    directional pad on the left of the map for one-thumb movement.
  - Co-op suppresses Mobile and touch-density layouts regardless of this
    setting. Touch is not an independently assignable co-op input device.
- **Manage Original Assets.** Re-run data import or pull in the original sound
  effects and/or monster art from your own copy of the game; already-imported
  originals can be switched on or off here too.
- **Cloud Saves**, under **Assets & Saves** — open the optional Google Drive
  synchronization screen described below.

### Gameplay options

- **Indestructible equipped items.** Exclude worn items from destruction by
  monsters, quicksand, and slime traps. Backpack items remain vulnerable and
  become more likely targets. This is not blanket protection against theft or
  every other way of losing an item.
- **Don't restore drained stats in town.** Off by default: ordinary paid town
  restoration remains the normal rule. When enabled, player stat drain persists
  through town visits and death. Recover the points with stat-boosting items,
  or disable the setting and return to town. This is separate from ordinary
  death's CON-loss fee.
- **Use library knowledge to filter available spells.** Off by default. Use
  perceived Library knowledge to hide spells believed incompatible with a
  monster's type or known immunities. Identification can be mistaken, so this
  is not perfect knowledge. Ordinary no-effect checks still operate with the
  option off.

### Controls at a glance

The game supports keyboard + mouse, **gamepad**, and **touch**:

- **Keyboard/mouse:** move and turn with the movement keys, act through the dungeon
  action strip, and navigate menus with the mouse or arrow/Enter.
- **Gamepad:** full controller navigation with focus highlighting; the analog
  sticks open **radial hotbar** wheels for quick actions, the d-pad moves and
  turns, and B/Esc backs out or cancels where supported.
- **Touch/phone:** tap the on-screen controls, use the on-screen d-pad for
  movement, and press-and-hold a button to preview what it does.

Local co-op uses **two gamepads**, or **one gamepad plus keyboard/mouse on PC**.
Touch is for solo play rather than a separately assignable second seat.

### Saving and checkpoints

Saving is automatic, but **not every action writes a save**. Checkpoints occur
on dungeon floor transitions, entering/leaving town, and hired-character
membership changes. Initial town entry after creating/loading a session also
establishes a checkpoint.

Ordinary room movement, same-floor relocations, shopping, item/gold exchange,
buffs, control switching, and monster-roster changes do not independently save.
Moving between town buildings is not a new town-entry checkpoint. These
changes are included in the next coherent checkpoint, which can be written in
the background.

Returning to the title, quitting normally, or suspending the app flushes the
latest local state. A **crash or forced termination** can lose changes since
the last completed checkpoint; do not force-close the process as a substitute
for the normal exit flow.

### Cloud Saves

Open **Settings → Assets & Saves → Cloud Saves** to enable optional Google
Drive synchronization and sign in. The screen shows synchronization status,
offers **Sync Now**, and lets you resolve conflicting local/cloud versions.
Read the conflict details before choosing which version to keep.

Cloud sync is **opt-in**, not a requirement for saving or playing. Local saves
remain usable offline, and turning cloud sync off leaves them intact.

## Differences from the original

- **Full controller and touch support.** These are remake additions — the original
  was a mouse-and-keyboard Windows game. The remake adds complete gamepad focus
  navigation with radial hotbar wheels, plus a phone-friendly touch layout.

[Back to Help index](index.md)
