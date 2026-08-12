; Inno Setup script for Mouse Share (Windows).
; Compiled with ISCC.exe (Inno Setup 6), preinstalled on GitHub's
; windows-latest runner image; see .github/workflows/ci.yml's
; build-installers job and installers/windows/build.ps1.
;
; Expects the release binaries to already be built at:
;   target\release\mouse-share-daemon.exe
;   target\release\mouse-share.exe          (the Tauri UI binary, once built)
;
; Run `cargo build --release --workspace` and `npm run tauri build` (from
; app/) before compiling this script locally; build.ps1 does both.

#define MyAppName "Mouse Share"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Mouse Share Contributors"
#define MyAppExeName "mouse-share.exe"
#define MyDaemonExeName "mouse-share-daemon.exe"

[Setup]
AppId={{B6C9F5E1-4C7A-4C8E-9C7E-8F2A9B1D4E60}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Per-user install by default so no admin prompt is required just to
; share a mouse and keyboard; the low-level input hooks this app installs
; do not require elevation on a normal desktop session.
PrivilegesRequired=lowest
OutputDir=output
OutputBaseFilename=MouseShare-Setup-{#MyAppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startupicon"; Description: "Start Mouse Share automatically when Windows starts"; GroupDescription: "Additional options:"; Flags: unchecked

[Files]
Source: "..\..\target\release\{#MyDaemonExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\Uninstall {#MyAppName}"; Filename: "{uninstallexe}"
Name: "{userstartup}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: startupicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Ensure the background daemon isn't left running (and holding its
; listening port) after uninstall.
Filename: "{cmd}"; Parameters: "/C taskkill /IM {#MyDaemonExeName} /F"; Flags: runhidden skipifdoesntexist; RunOnceId: "StopDaemon"
