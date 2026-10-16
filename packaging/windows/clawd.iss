; Inno Setup script for the Rust build: per-user install, no admin rights needed.
; Same installer contract as the Python app (packaging\windows\clawd.iss) so that the 0.1.x
; self-update installs it over the old one: same AppId, same folder, same silent flags.
;   iscc /DAppVersion=0.2.0 /DExeDir=<dir with Clawd.exe> packaging\windows\clawd.iss
; (packaging\build_windows.ps1 does the cargo build and passes both.)
; No [Code] section and no WebView2 bootstrapper, like the Python installer: the runtime is
; assumed present (Windows 11 ships it); without it the tray still runs.
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExeDir
  #define ExeDir "..\..\target\release"
#endif

[Setup]
; Do not change: the installed Python 0.1.x app is recognised by this id (one Add/Remove entry).
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
SetupIconFile=..\..\app\icons\icon.ico
UninstallDisplayIcon={app}\Clawd.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=force

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"

[InstallDelete]
; Installing over the PyInstaller build: remove what only it shipped. PyInstaller 6 keeps
; everything in _internal; the loose names are for older one-folder builds. The C runtime DLLs
; (VCRUNTIME140, ucrtbase, api-ms-win-*) are left alone on purpose: the Rust exe may load them.
Type: filesandordirs; Name: "{app}\_internal"
Type: files; Name: "{app}\base_library.zip"
Type: files; Name: "{app}\python*.dll"
Type: files; Name: "{app}\*.pyd"
Type: files; Name: "{app}\libcrypto*.dll"
Type: files; Name: "{app}\libssl*.dll"

[Files]
Source: "{#ExeDir}\Clawd.exe"; DestDir: "{app}"; Flags: ignoreversion
#ifdef ResourcesDir
; Private build only (CLAWD_PRIVATE_FIRMWARE=1): firmware\ and renderer\ next to the exe, where
; clawd_firmware::resources_dir() looks. A public build has no ResourcesDir.
Source: "{#ResourcesDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion
#endif

[Icons]
Name: "{userprograms}\Clawd"; Filename: "{app}\Clawd.exe"

[Run]
Filename: "{app}\Clawd.exe"; Description: "Start Clawd"; Flags: nowait postinstall

[UninstallRun]
; Stop the tray app, then remove autostart and the Claude Code hooks it added.
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM Clawd.exe"; Flags: runhidden; RunOnceId: "StopClawd"
Filename: "{app}\Clawd.exe"; Parameters: "cleanup"; Flags: runhidden waituntilterminated; RunOnceId: "CleanupClawd"
