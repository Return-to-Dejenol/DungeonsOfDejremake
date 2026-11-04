; Inno Setup script for the Windows installer. Build it with scripts/build-installer.ps1.
; It ships the game and the GeneralUser GS SoundFont only: players import their own copy
; of the original game on first run (saves and data go to %LOCALAPPDATA%\DejenolRestoration).

#define AppName "Dejenol Restoration Project"
#define Exe "DejenolRestoration.exe"
#define AppVersion "0.1.0"

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
InfoBeforeFile=..\DISCLAIMER.md
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#Exe}
SetupIconFile=..\assets\icon.ico

[InstallDelete]
; What the project installed under its earlier name.
Type: files; Name: "{app}\DejenolRemake.exe"
Type: filesandordirs; Name: "{autoprograms}\Depths of Dejenol Remake"
Type: files; Name: "{autodesktop}\Depths of Dejenol Remake.lnk"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\target\release\ddj-game.exe"; DestDir: "{app}"; DestName: "{#Exe}"; Flags: ignoreversion
Source: "..\vendor\soundfont\GeneralUser-GS.sf2"; DestDir: "{app}\sound"
Source: "..\vendor\soundfont\LICENSE.txt"; DestDir: "{app}\sound"; DestName: "GeneralUser-GS-LICENSE.txt"
Source: "..\patches\modern.toml"; DestDir: "{app}"; DestName: "modern-rebalance-reference.toml"
Source: "..\docs\balance.md"; DestDir: "{app}"; DestName: "rules.md"
Source: "..\DISCLAIMER.md"; DestDir: "{app}"

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#Exe}"; WorkingDir: "{app}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#Exe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#Exe}"; Description: "Play now"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent
