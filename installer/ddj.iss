; Inno Setup script for the Windows installer. Build it with scripts/build-installer.ps1.
; It ships the game and the GeneralUser GS SoundFont only: players import their own copy
; of the original game on first run (saves and data go to %LOCALAPPDATA%\DejenolRemake).

#define AppName "Depths of Dejenol Remake"
#define AppVersion "0.1.0"

[Setup]
AppId={{6B2F3D61-6E0C-4C1F-9B1D-6A9A0C3E7D42}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=Silvaria
DefaultDirName={localappdata}\Programs\DejenolRemake
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
OutputDir=..\target\installer
OutputBaseFilename=DejenolRemake-Setup-{#AppVersion}
InfoBeforeFile=..\DISCLAIMER.md
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\target\release\ddj-game.exe"; DestDir: "{app}"; DestName: "DejenolRemake.exe"; Flags: ignoreversion
Source: "..\vendor\soundfont\GeneralUser-GS.sf2"; DestDir: "{app}\sound"
Source: "..\vendor\soundfont\LICENSE.txt"; DestDir: "{app}\sound"; DestName: "GeneralUser-GS-LICENSE.txt"
Source: "..\patches\modern.toml"; DestDir: "{app}"; DestName: "modern-rebalance-reference.toml"
Source: "..\docs\balance.md"; DestDir: "{app}"; DestName: "rules.md"
Source: "..\DISCLAIMER.md"; DestDir: "{app}"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\DejenolRemake.exe"; WorkingDir: "{app}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\DejenolRemake.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\DejenolRemake.exe"; Description: "Play now"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent
