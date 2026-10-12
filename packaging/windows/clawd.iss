; Inno Setup script: per-user install, no admin rights needed.
; iscc /DAppVersion=0.1.0 packaging\windows\clawd.iss  (after PyInstaller made dist\Clawd)
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}
AppName=Clawd
AppVersion={#AppVersion}
AppPublisher=Clawd
DefaultDirName={localappdata}\Programs\Clawd
DefaultGroupName=Clawd
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename=Clawd-{#AppVersion}-windows-setup
SetupIconFile=..\..\build\icons\clawd.ico
UninstallDisplayIcon={app}\Clawd.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=force

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\..\dist\Clawd\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion

[Icons]
Name: "{userprograms}\Clawd"; Filename: "{app}\Clawd.exe"

[Run]
Filename: "{app}\Clawd.exe"; Description: "Start Clawd"; Flags: nowait postinstall

[UninstallRun]
; Stop the tray app, then remove autostart and the Claude Code hooks it added.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM Clawd.exe"; Flags: runhidden; RunOnceId: "StopClawd"
Filename: "{app}\Clawd.exe"; Parameters: "cleanup"; Flags: runhidden waituntilterminated; RunOnceId: "CleanupClawd"
