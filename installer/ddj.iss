; Inno Setup script for the Windows installer. Build it with scripts/build-installer.ps1.
; It ships the game and the GeneralUser GS SoundFont only: players import their own copy
; of the original game on first run (saves and data go to %LOCALAPPDATA%\DejenolRestoration).
; It looks like the original's own setup (the VB3 setup kit): a full-screen backdrop with a
; title panel and a welcome panel, and one small "Install To" dialog.

#define AppName "Dejenol Restoration Project"
#define Exe "DejenolRestoration.exe"
; The version comes from crates/game/Cargo.toml (scripts/build-installer.ps1 passes it).
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6B2F3D61-6E0C-4C1F-9B1D-6A9A0C3E7D42}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Silvaria
DefaultDirName={localappdata}\Programs\DejenolRestoration
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\target\installer
OutputBaseFilename=DejenolRestoration-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#Exe}
SetupIconFile=..\assets\icon.ico
; The original's look: a backdrop behind a small classic dialog that asks only where.
WizardStyle=classic
WizardSizePercent=100
WizardSmallImageFile=..\assets\setup-small.bmp
WizardImageFile=..\assets\setup-side.bmp
DisableWelcomePage=yes
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=yes
AppendDefaultDirName=no

[Messages]
SelectDirLabel3=If you want to install the [name] in a different directory and/or drive, type the name of the directory.
SelectDirBrowseLabel=To quit Setup, choose the Cancel button.
ButtonInstall=&Continue
ButtonNext=&Continue

[InstallDelete]
; What the project installed under its earlier name.
Type: files; Name: "{app}\DejenolRemake.exe"
Type: filesandordirs; Name: "{autoprograms}\Depths of Dejenol Remake"
Type: files; Name: "{autodesktop}\Depths of Dejenol Remake.lnk"

[Files]
Source: "..\target\release\ddj-game.exe"; DestDir: "{app}"; DestName: "{#Exe}"; Flags: ignoreversion
Source: "..\vendor\soundfont\GeneralUser-GS.sf2"; DestDir: "{app}\sound"
Source: "..\vendor\soundfont\LICENSE.txt"; DestDir: "{app}\sound"; DestName: "GeneralUser-GS-LICENSE.txt"
Source: "..\patches\modern.toml"; DestDir: "{app}"; DestName: "modern-rebalance-reference.toml"
Source: "..\docs\balance.md"; DestDir: "{app}"; DestName: "rules.md"
Source: "..\DISCLAIMER.md"; DestDir: "{app}"
Source: "..\assets\setup-small.bmp"; Flags: dontcopy

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#Exe}"; WorkingDir: "{app}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"

[Run]
Filename: "{app}\{#Exe}"; Description: "Play now"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent
; The game's updater runs Setup silently: the game starts again once it's done.
Filename: "{app}\{#Exe}"; WorkingDir: "{app}"; Flags: nowait; Check: WizardSilent

[Code]
function GetSystemMetrics(Index: Integer): Integer; external 'GetSystemMetrics@user32.dll stdcall';

var
  Backdrop: TSetupForm;

{ A light panel across the backdrop with centred text, as the original's setup had. }
procedure Banner(Top, Height: Integer; Text, FontName: String; Size: Integer);
var
  P: TPanel;
  L: TLabel;
begin
  P := TPanel.Create(Backdrop);
  P.Parent := Backdrop;
  P.SetBounds(ScaleX(8), Top, Backdrop.ClientWidth - ScaleX(16), Height);
  P.BevelOuter := bvLowered;
  P.Color := clBtnFace;
  L := TLabel.Create(Backdrop);
  L.Parent := P;
  L.Align := alClient;
  L.Alignment := taCenter;
  L.Layout := tlCenter;
  L.WordWrap := True;
  L.Font.Name := FontName;
  L.Font.Size := Size;
  L.Font.Style := [fsBold];
  L.Caption := Text;
  L.Transparent := False;
  L.Color := clBtnFace;
end;

{ The original's full-screen backdrop: dark, a title panel at the top, a welcome panel at
  the bottom; the small Setup dialog sits in front of it. }
procedure InitializeWizard;
var
  W, H: Integer;
begin
  if WizardSilent then
    exit;
  W := GetSystemMetrics(0);
  H := GetSystemMetrics(1);
  Backdrop := CreateCustomForm(W, H, False, False);
  Backdrop.BorderStyle := bsNone;
  Backdrop.Color := $202020;
  Backdrop.Caption := '{#AppName} Setup';
  Backdrop.SetBounds(0, 0, W, H);
  Banner(ScaleY(8), ScaleY(40), '{#AppName} Setup Program', 'Times New Roman', 20);
  Banner(H - ScaleY(128), ScaleY(120),
    'Welcome to the {#AppName} installation program!' + #13#13 +
    'A fan remake of Mordor: The Depths of Dejenol, not affiliated with its rights holders.' + #13 +
    'It starts with the free shareware release (dungeon levels 1-3);' + #13 +
    'import your own copy of the original for all fifteen.' + #13 +
    '- The original''s own windows, read from your copy -' + #13 +
    '- Its Sound Canvas music and sound effects -' + #13 +
    '- A party of your other characters, guided by macros -', 'MS Sans Serif', 8);
  Backdrop.Show;
  { The dialog as the original's: no header, the M where it had its dwarf, no disk space line. }
  H := WizardForm.MainPanel.Height + WizardForm.Bevel1.Height;
  WizardForm.MainPanel.Visible := False;
  WizardForm.Bevel1.Visible := False;
  WizardForm.InnerNotebook.Top := WizardForm.InnerNotebook.Top - H;
  WizardForm.ClientHeight := WizardForm.ClientHeight - H - ScaleY(40);
  WizardForm.DiskSpaceLabel.Visible := False;
  ExtractTemporaryFile('setup-small.bmp');
  WizardForm.SelectDirBitmapImage.Bitmap.LoadFromFile(ExpandConstant('{tmp}\setup-small.bmp'));
  WizardForm.SelectDirBitmapImage.SetBounds(0, 0, ScaleX(55), ScaleY(55));
  WizardForm.SelectDirLabel.Left := ScaleX(63);
  WizardForm.SelectDirLabel.Width := WizardForm.SelectDirPage.ClientWidth - ScaleX(63);
end;

procedure DeinitializeSetup;
begin
  if Backdrop <> nil then
    Backdrop.Free;
end;
