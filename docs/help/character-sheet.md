# Character Sheet

The character sheet is where you check your hero's stats, manage equipment and
inventory, study your spellbook, and review your companions.

[Back to Help index](index.md)

## How to play

In the dungeon, click or tap your **name/stats card**, or press **RB / R1** on
your gamepad, to open the sheet for your currently controlled character.
Each co-op player's card opens that player's character, including after a
control switch. In town, the status card opens details; its name dropdown is
reserved for [switching characters](town.md#switching-characters).

### Stats and combat numbers

Your hero has six stats — **STR, INT, WIS, CON, CHA, DEX** — that drive everything
else:

- **Attack / Defense (A/D).** Your A/D comes from three sources added together:
  your stats, your equipped items, and your **single best** guild (only one guild
  contributes A/D). Stats only affect A/D when they're unusually high or low — each
  point of STR or DEX above the average range adds +2 attack and +2 defense (and
  subtracts below it), while INT and WIS add +1/+1 per point outside their own
  average range.
- **Spell Points (SP).** SP = INT×5 + WIS×5 using your natural stats after
  temporary drain; item INT/WIS bonuses do **not** raise maximum SP. Spend SP to
  cast; it refills to full when you return to town (certain items can also restore
  it), but it does not gradually regenerate while you rest in the dungeon.
- **HP.** Your maximum HP is set by your CON and guild levels (see the [Guild
  Hall](guild-hall.md)). CON drain can reduce it; item CON bonuses do not raise it.

Your **banked gold** is shown here too, so you can check your funds at a glance.

### Equipment

Equip weapons, armor, and accessories to boost your A/D, stats, resistances, and
abilities. Key slots:

- A **weapon** slot and an **off-hand / shield** slot. You can wield a one-handed
  weapon, a one-handed weapon **plus** a shield, or a single two-handed weapon.
- Body and accessory slots for armor, rings, and other gear.

Mind each item's requirements — some gear needs minimum stats or membership/levels
in particular guilds. Alignment affects equipment bonuses and opposite-alignment
gear can be cursed. Cursed equipped items can lock to you until you remove the
curse at the [General Store](general-store.md). Attached miscellaneous items
also have paid removal through the character page or Bank.

### Inventory and spells

- Your **inventory** lists everything you're carrying that isn't equipped. From
  here you can equip, use, drop, or inspect items.
- Your **spellbook** lists the spells you know. To cast, choose a spell, then a
  legal target when the spell requires one. Offensive spells use the selected
  monster group; healing, cures, buffs, and raises each have their own eligible
  recipients rather than all sharing one unrestricted target list.
  How effective a shared spell is scales with your **highest** level among the
  guilds that teach it — so a spell stays strong even if your active guild is a
  weaker teacher of it.
- Learned **Control, Restrain, and Domination** are selected from a particular
  monster companion's dungeon context menu, not the generic Cast list or spell
  hotbar. Their learned descriptions and AI permissions remain available here.
- **Spell-bearing items** use recipient, destination, or search pickers as
  appropriate, whether activated from inventory, equipped gear, or an item
  hotbar slot. Canceling or rejecting an invalid target does not consume a
  charge. Ordinary non-spell stat/SP consumables remain user-only.

### Automatic buffs

Status buffs such as levitation, invisibility, protection, and resistances use
three-state controls, not an Auto checkbox:

| Mode | Automatic casting permission |
|------|------------------------------|
| Disabled | Do not cast this buff automatically; manual casting remains available. |
| Self | Cast when the caster needs it. A mass spell still has its normal collateral coverage. |
| All | May also cast to help eligible co-located party characters. |

The separate **Automatic buffs** master in the spellbook (**Buffs** in ally AI)
blocks **all** automatic buff casts by that character, including self-buffs.
It retains the individual modes and does not prevent manual casting or
receiving a buff from someone else.

Played characters, saved-character AI allies, and generated allies share these
rules. Outside hostile combat, support opportunities coordinate useful buffs
across the characters present, including co-op parties together in one room.
Useful cheaper mass coverage can precede self-casts, followed by All-mode
helpers. Buffs cost normal SP and can exhaust the caster's remaining SP; they
have no special healing reserve.

During hostile combat there is no free shared player auto-buff phase.
AI-controlled characters may choose legal buffs on ordinary turns. Changing
a mode or permission does not itself cast a spell.

### Ally AI and inspection

Open a character ally's **Inspect / AI behavior** menu to review their equipment,
learned spells, and these capabilities:

- **Tank** and **Attack** permit those AI combat actions.
- **Offensive cast** permits offensive learned spells, including appropriate
  typed Banish/Dispel spells. These respect actual damage, type, resistance,
  immunity, and target limits; they are not unconditional kills.
- **Healing / cures** permits helping other eligible recipients. Turning it
  off restricts healing/cures to self; it does not control buffs.
- **Buffs** is the independent automatic-buff master described above.
- **Charm** permits hostile-monster recruitment and out-of-combat binding
  maintenance. It does not automatically provoke peaceful monsters.
- **Thieving** permits automatic physical chest opening/disarming. Magical
  unlocking has its own learned-spell permission.

Individual non-buff spell switches further restrict automatic use. Manual-only
spells stay visible without a misleading automation switch. AI allies do not
initiate item spells.

Inspection is organized as **Equipment**, **AI behavior**, **Spells**, then
**Stats**. Spell rows place permission/mode before the aligned SP cost.
Stats includes combat/guild information, **Attributes**, multiline **Status**,
and **Resistances**, in that order. Status combines equipped abilities and active
conditions without duplicate entries. Saved equipment respects identification;
generated Tavern gear is not normally exposed. Gamepad navigation can scroll
the informational tail without activating settings.

Monster companions instead show their own
[binding/contentment details](companion-store.md#binding-and-contentment).

### Town Exchange

In town, choose **Exchange** to give items or **bank gold** between two eligible
active real saved characters. This works within a solo party or across either
co-op party, including AI-controlled saved allies. Generated Tavern adventurers,
unhired characters, and inactive members never appear as exchange candidates.

Choose the two characters using the title-like name dropdowns. Equipment is
grouped by slot above **Unequipped Inventory**. An equipped item must first be
unequipped by its owner before it can be given; normal curse and capacity
restrictions still apply.

Select a character's **Gold** row to show the central proportional amount slider
and **Give** action. The readout shows selected/available bank gold with thousands
separators, for example **12,500 / 248,675 gold**. Slider endpoints select zero
or the full balance. Gold gifts are direct bank transfers: no income split or
generated-ally fee applies.

Only the player owning a character's party can browse that character's
inventory, adjust its gold amount, or give its possessions. Ownership follows
the party, not the screen column or whether the character is currently
AI-controlled. One player can therefore operate both columns for a same-party
pair; each controls only their own source in a cross-party pair. Giving is
direct and does not require the recipient to approve a trade.

**Gamepad controls** apply only to characters owned by your player:

| Control | Action |
|---------|--------|
| LB / L1 or RB / R1 | Toggle between character selection and inventory for your column. |
| D-pad in character-selection mode | Cycle the character shown in that column. |
| Up / Down in inventory | Browse rows, including Gold. |
| Left / Right in inventory | Adjust gold when its slider is shown; on an item, switch columns if you own both. |
| X / Square | Equip or unequip the selected item. |
| Y / Triangle | Give the selected item or gold amount. |
| A / Cross | Equip/unequip the selected item, or **give the selected gold amount** when Gold is shown. |
| B / Circle | Leave Exchange. |

**A / Cross on Gold executes Give immediately**; it does not merely focus the
slider or open a confirmation. Check the selected amount before pressing it.

## Differences from the original

- **Equipped gear is separated from your pack.** Your equipped items are shown
  apart from your carried inventory, so it's clear at a glance what you're wearing
  versus what you're hauling.
- **One consistent spell-effectiveness rule.** All of your spells — damage and
  healing alike — scale off your highest level among the guilds that teach them.
  In the original, healing scaled off your *active* guild instead, so a heal could
  get weaker just because you'd switched guilds. Here it stays consistent.

[Back to Help index](index.md)
