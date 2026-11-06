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
  and other systems fall back to egui's fonts, which is close but not exact.

## Where the remake's own features went

The rule: in an original window where one fits, else a new window in the same style.
Taken while building; reviewed at play-test.

- **Import Game...**: the Options window, in the slot of its hidden "Screen Saver..."
  button (the original kept its data path in Options-related INI settings).
- **Party macros**: the Party window's Options button opens the macro editor, a remake
  window (the original's Party Options only chose one default action).
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

## Not done yet

- Buffers 1-10 and Pickup/Drop look as in the original but only say they aren't in the
  remake yet; Give, Carry, Combine, Realign, the
  Library, the Guild Log, Exchange Items, Change Password and the Control Panel are
  disabled.
- The spell list's type filter only offers "All"; the Resist. and Look tabs are sparse.
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
