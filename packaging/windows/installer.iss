#define AppName "GuluTwist Break Station"
#define AppVersion "0.1.0"
[Setup]
AppId=app.gulutwist.breakstation
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\GuluTwist
DefaultGroupName={#AppName}
PrivilegesRequired=lowest
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=GuluTwist-0.1.0-Windows-x64-Setup
SetupIconFile=..\..\assets\icons\app.ico
UninstallDisplayIcon={app}\app.ico
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
[Files]
Source: "..\..\target\x86_64-pc-windows-msvc\release\gulu-twist.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\icons\app.ico"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\gulu-twist.exe"; IconFilename: "{app}\app.ico"; AppUserModelID: "app.gulutwist.breakstation"
Name: "{group}\卸载 {#AppName}"; Filename: "{uninstallexe}"
[Run]
Filename: "{app}\gulu-twist.exe"; Description: "打开咕嘟扭扭放风站"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\app.gulutwist.breakstation"; Flags: uninsdeletekey
