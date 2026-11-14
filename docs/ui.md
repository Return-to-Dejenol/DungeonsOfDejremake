# The original's windows

Decided 2026-11-04: what the player sees should look exactly like the original.

## How

- **Layouts come from the player's own MORDOR.EXE.** It is a Visual Basic 3 program and
  still holds its 32 forms (frmMenu, frmCityMain, frmDungeonMain, frmCharStat1...):
  every control's type, position, caption, colours, font and picture. The importer
  reads them (`crates/extract/src/forms.rs`) into `FORMS.JSON` beside the data, with
  each form's pictures as `<form>_<n>.BMP`. Nothing from them is in the repo or the
  builds. `vb3props.txt` lists each VB3 control class's property types, as VBRUN300
  and the VBXs define them; it is interface metadata, not Mordor's.
- **An 800x600 Windows 95 desktop, scaled evenly.** The original was designed for
  800x600 ("Adjust to Display" scaled its windows by screen/800x600). The remake draws
  the maximized main window, its toolbar and MDI area at that size and scales the whole
  desktop to the screen with sharp pixels (`crates/game/src/win.rs`). Windows move by
  their title bars; positions are not saved yet.
- **Default arrangement** as the original's sub14B4: the character window top left, the
  city or dungeon beside it, Objects & Spells under that, the party under the
  character, the AutoMap beside the party, Messages bottom right.
- **Run-time layout** the forms don't hold (rows of stats, party members, store items,
  buffer buttons) is ported from each form's Form_Load/Form_Resize code.
- **Text** uses the system's own MS Sans Serif bitmap font (`sserife.fon`) when there is
  one (Windows), drawn pixel for pixel; bold is drawn twice a pixel apart, as Windows
  did. Courier New, Arial and Times New Roman use the system's TrueType files. The web
  and Android draw MS Sans Serif and Arial with Liberation Sans and Times New Roman with
  Liberation Serif (assets/fonts, SIL OFL), regular, bold and the Main Menu's bold
  italic; close, not exact.
- **Touch screens** (2026-11-10): two fingers pinch the whole desktop up to 3x and move
  it about; pinching back out fits it again. Buttons, check boxes and option buttons
  take taps 5 pixels past their edges.
- **The phone shell** (Android only, 2026-11-13; the owner chose it over a touch-sized
  desktop). The same windows, drawn the same way, but placed for a phone instead of on
  the 800x600 desktop (`stats::shell`, `Desk.place`). Windows and the web keep the
  desktop; `DDJ_SHELL=1` shows the shell on Windows for testing.
  - Portrait: the main window's title and toolbar; the City or Dungeon window (or the
    open building) and Messages, full width; a tab strip (Character, Objects, Party),
    with the tab's windows below it as big as they fit (Party shares its tab with the
    AutoMap in the dungeon); in the dungeon a touch pad at the bottom.
  - Landscape: the play window, Messages and the pad on the left; the tabs on the right.
  - The touch pad is a remake window in the original's style. It has the Dungeon window's
    own arrows, Take, Open, Fight and Spell, finger-sized, plus Defend and Run (the
    keyboard's D and R). Defend reads Join while a group offers to join (the keyboard's
    J). The toolbar's and the dungeon's buttons that made a window
    active (Objects & Spells, the AutoMap) bring its tab to the front.
  - Dialogs and the HelpLesson open centred, as big as they fit; the HelpLesson's main
    window fills the screen. The on-screen keyboard lifts a dialog so the box typed in
    stays above it. A tap on the box brings the keyboard back after the system hides
    it, and a tap elsewhere puts it away. Windows don't move by their title bars, and
    there is no pinch zoom.
  - The system's navigation bar is kept clear.
  - Message boxes that only tell ("Not enough gold!", BUY ITEM) show while playing as a
    yellow banner in the style of a Windows 95 tooltip, for a few seconds or until
    tapped. Their words also go into Messages. Boxes that ask stay modal.
- **Touch gestures** (any touch screen): a swipe across the maze or the monster box
  steps (up), turns (left, right) or turns about (down). Dragging a list or a scrolled
  strip moves it, with a flick that carries on: Messages, the guild and bank lists,
  Objects & Spells' items and spells, the store's and Confinement's stock, and HelpLesson
  topics. A drag that starts upright on an item scrolls; one that starts sideways carries
  the item (to the store's boxes). Combo lists open with finger-tall rows, above the box
  when there's no room below.
- **The Symbol font's arrows** (the dungeon's movement buttons) are drawn from
  Liberation Sans off Windows; before 2026-11-13 they showed as empty boxes on the web
  and Android.

## Where the remake's own features went

The rule: in an original window where one fits, else a new window in the same style.
Taken while building; reviewed at play-test.

- **Import Game...**: the Options window, in the slot of its hidden "Screen Saver..."
  button (the original kept its data path in Options-related INI settings).
- **Party macros**: the Party window's Options button opens the macro editor, a remake
  window (the original's Party Options only chose one default action). At most 12 rules
  of up to 3 conditions; casting names one of the character's spells.
- **Party order**: the Party window's < and > show the previous or next member, and Up
  and Down move the shown member among the followers (not in a fight), between Options
  and a narrower Leave.
- **Adding an ally**: the toolbar's add-character button ('A' in the original) opens the
  original's "Pickup Character" window with the saved characters.
- **The Seer**: its Character/Monster/Item buttons ask through the same window.
- **Morgue costs**: asked in a message box before raising or hiring rescuers.
- **Store stock**: the column the original headed "U G N E" shows the shared stock
  count, headed "Stock"; Confinement's likewise.
- **Items to services**: drag an item from Objects & Spells onto the store's Uncurse or
  Identify box (or a companion onto Confinement's Bind or Sell box), as in the original.
- **HelpLesson**: MORDOR.HLP (imported; else the bundled shareware's) in a Windows Help
  3.1 window: Contents, Search, Back, History, << and >>, the help's fonts, green jumps,
  pictures, tables and its glossary, tutorial and statistics windows. The menu and the
  toolbar's "?" open the contents; F1 the topic for the window in front (its
  HelpContextID); character creation's Race/Guild Stats and Tutorial their own windows.
  No menu bar or printing; a secondary window widens to show a whole table, where
  WinHelp let the player resize it.

- **Guilds** (2026-11-09) speak the original's words, read from MORDOR.EXE
  (formats.md "The original's text"): the welcome and refusals, "Join"/"Re-Acquaint",
  the join question with its cost, Exp. Info, and the make-level boxes. The list shows
  only the guilds that take the character's race and alignment, with its levels. Each
  guild's Guildmaster starts as the original's (Crashland, level 64, for the Nomads...);
  a character who makes a higher level takes over, in `hall.json`, and gets the guild's
  Crest. The original's guild log and the forfeit of a quest when pinned aren't built.

- **Messages** (2026-11-09) are the original's words wherever the remake does what the
  original did, read from MORDOR.EXE like the guilds'. As the original, the dungeon
  window's lblCharacterHit, lblMonsterHit and lblSquareInfo tell the controlled
  character's news as "You hit for 7 damage!", "2 Zombies hit for 13!", "You have fallen
  into a pit for 5 hits."; the Messages window keeps a line for every member (the
  original's named form where it has one, else the quoted name and the "You" words, and
  the remake's own lines for fight actions). The stores', morgue's, Holding's and Seer's
  refusals and questions are the original's titled message boxes ("Not enough gold!",
  BUY ITEM). Lines for what the original did silently or showed elsewhere are gone
  (descending, stairs, a cleared room, a sale, healing, buffs); the remake's own
  features keep theirs. Also the original's: trap and ability names, full stat names,
  monster plurals (fn0E23), "Character 2 of 3", the Hall of Records' words and its
  holders, the creation and delete boxes, the main window's lines.
- **Not matched yet**, where the remake's rules differ: the Seer's per-target fees and
  its asking first, rescue by coordinates, raising when the dead character is run, Exit
  from anywhere, level titles in lblGuildInfo, the character list's level, guild and
  status, item alignment markers, "points left" at creation, the spell verbs by kind
  ("Fried", "Froze"), the guild log, and the bank's item slots.

## Not done yet

- Buffers 1-10 and Pickup/Drop look as in the original but only say they aren't in the
  remake yet; Give, Carry, Combine, Realign, the
  Library, the Guild Log, Exchange Items, Change Password and the Control Panel are
  disabled.
- The spell list's type filter only offers "All"; the Look tab is sparse.
- Window positions are not saved per character as the original saved them.
- Not matched yet: the store's inner sunken list boxes, its item order and per-alignment
  stock ("0 0 1 0"); Windows 95 title bars are drawn from memory
  (the comparison host draws Windows 11 ones).

## Checked against the original (2026-11-04)

v1.1 ran on sereneblossomns under otvdm, its windows resized to their design sizes so
their fonts came back to design size, and each was compared with the remake at 800x600.
What that settled:

- Font flags: VB's own controls default to bold, the VBXs' to regular; a stored font
  is bold when bit 0 or 7 is set, italic with bit 1 (drawn sheared, as Windows did).
- Menu: no toolbar; "Create a Character" / "Load Character"; loading (through the
  Pickup Character window) turns them into Run "Name" / Unload Character and shows
  Exchange, Password and Delete. HelpLesson and Hall of Records look enabled.
- The MDI area is tiled with the stone picture kept on frmMain's picPalette.
- Toolbar: "You have a total of N Gold." and "You need N more experience for level L."
  at its right; numbers have thousands separators everywhere.
- Character window: the name in quotes; two rows of four equal tabs, the current tab's
  row in front and only its caption bold; "Nomad (1)", "You are Alone".
- Party: names and hits green, status "OK" black, option "None".
- Objects & Spells: forty numbered slots; the newest message is selected.
- Dungeon: "9,11,1 North [ 1 ]"; every button looks enabled; the AutoMap fills its
  window, walls in white on gray, the level in its caption.
- Labels with a border are sunken (VBCTL3D), THREED buttons have a black outline.
