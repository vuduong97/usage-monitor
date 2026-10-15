; Minimal Inno Setup script for the phase 01 spike: only to measure the installer size of a single
; Tauri exe with LZMA2, like packaging/windows/clawd.iss does for the Python app.
;   iscc desktop-rs\spike\spike.iss          (after: cd desktop-rs\spike; cargo build --release)
; Output: desktop-rs\spike\target\installer\clawd-spike-setup.exe
[Setup]
AppId={{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F4}
AppName=Clawd Spike
AppVersion=0.0.0
DefaultDirName={localappdata}\Programs\ClawdSpike
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir=target\installer
OutputBaseFilename=clawd-spike-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "target\release\clawd-spike.exe"; DestDir: "{app}"; Flags: ignoreversion
