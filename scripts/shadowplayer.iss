; ShadowPlayer Inno Setup Script
; Builds a Windows installer for ShadowPlayer with desktop integration and file associations.

#define MyAppName "ShadowPlayer"
#ifndef MyAppVersion
#define MyAppVersion "0.30.0"
#endif
#define MyAppPublisher "ShadowPlayer Developers"
#define MyAppURL "https://github.com/Sudipta615/ShadowPlayer"
#ifndef MyAppExeName
#define MyAppExeName "shadowplayer.exe"
#endif
; `package-windows.sh` overrides these for cross-target builds. The defaults
; preserve the original in-tree layout.
#ifndef MyAppBinary
#define MyAppBinary "..\target\release\shadowplayer.exe"
#endif
#ifndef MyAppRemote
#define MyAppRemote "..\target\release\shadowplayer-remote.exe"
#endif
#ifndef MyAppArch
#define MyAppArch "x86_64"
#endif
#ifndef MyAppArm64
#define MyAppArm64 0
#endif

[Setup]
AppId={{5E285F5D-C35B-4C6A-91E2-F4A5E6B7C8D9}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
OutputDir=..\target\distrib
OutputBaseFilename=shadowplayer-{#MyAppVersion}-setup-{#MyAppArch}
SetupIconFile=..\crates\app\assets\icon.ico
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
#if MyAppArm64 == 1
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associate_stm"; Description: "Associate .stm theme files with ShadowPlayer"; GroupDescription: "File Associations:"
Name: "associate_audio"; Description: "Associate lossless audio files (.flac, .wav, .dsf, .cue) with ShadowPlayer"; GroupDescription: "File Associations:"; Flags: unchecked

[Files]
Source: "{#MyAppBinary}"; DestDir: "{app}"; Flags: ignoreversion
#if FileExists(MyAppRemote)
Source: "{#MyAppRemote}"; DestDir: "{app}"; Flags: ignoreversion
#endif
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\crates\app\assets\icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icon.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Registry]
; .stm theme association
Root: HKA; Subkey: "Software\Classes\.stm"; ValueType: string; ValueName: ""; ValueData: "ShadowPlayer.Theme"; Flags: uninsdeletevalue; Tasks: associate_stm
Root: HKA; Subkey: "Software\Classes\ShadowPlayer.Theme"; ValueType: string; ValueName: ""; ValueData: "ShadowPlayer Theme"; Flags: uninsdeletekey; Tasks: associate_stm
Root: HKA; Subkey: "Software\Classes\ShadowPlayer.Theme\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\icon.ico,0"; Tasks: associate_stm
Root: HKA; Subkey: "Software\Classes\ShadowPlayer.Theme\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associate_stm

; Lossless audio associations
Root: HKA; Subkey: "Software\Classes\.flac\OpenWithProgids"; ValueType: string; ValueName: "ShadowPlayer.Audio"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate_audio
Root: HKA; Subkey: "Software\Classes\.dsf\OpenWithProgids"; ValueType: string; ValueName: "ShadowPlayer.Audio"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associate_audio
Root: HKA; Subkey: "Software\Classes\ShadowPlayer.Audio"; ValueType: string; ValueName: ""; ValueData: "Audio File"; Flags: uninsdeletekey; Tasks: associate_audio
Root: HKA; Subkey: "Software\Classes\ShadowPlayer.Audio\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associate_audio

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

