#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{C8E08820-26ED-4F14-A672-4FC8F7A3A429}
AppName=PaintAtlas
AppVersion={#AppVersion}
AppPublisher=Krueels
AppPublisherURL=https://github.com/Krueels/PaintAtlas
AppSupportURL=https://github.com/Krueels/PaintAtlas/issues
AppUpdatesURL=https://github.com/Krueels/PaintAtlas/releases
DefaultDirName={localappdata}\Programs\PaintAtlas
DefaultGroupName=PaintAtlas
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=PaintAtlas-Setup-windows-x86_64
SetupIconFile=..\..\assets\PaintAtlas.ico
UninstallDisplayIcon={app}\paintatlas.exe
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"; Flags: unchecked

[Files]
Source: "..\..\target\x86_64-pc-windows-msvc\release\paintatlas.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\default_brushes\*"; DestDir: "{app}\assets\default_brushes"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\PaintAtlas"; Filename: "{app}\paintatlas.exe"
Name: "{autodesktop}\PaintAtlas"; Filename: "{app}\paintatlas.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\paintatlas.exe"; Description: "Launch PaintAtlas"; Flags: postinstall nowait skipifsilent
