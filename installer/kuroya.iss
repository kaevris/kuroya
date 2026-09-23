#ifndef SourceRoot
  #error SourceRoot is required
#endif

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

#define SourceFileProgId "Kuroya.SourceFile"

[Setup]
AppId={{B7ADF221-E903-4075-8A67-2DE905EF5A31}
AppName=Kuroya
AppVersion={#AppVersion}
AppVerName=Kuroya {#AppVersion}
AppPublisher=Kuroya Contributors
AppPublisherURL=https://github.com/redmarklabscom/kuroya
AppSupportURL=https://github.com/redmarklabscom/kuroya/issues
AppUpdatesURL=https://github.com/redmarklabscom/kuroya/releases
AppCopyright=Copyright 2026 Kuroya Contributors
VersionInfoVersion={#AppVersion}
VersionInfoCompany=Kuroya Contributors
VersionInfoDescription=Kuroya Setup
VersionInfoProductName=Kuroya
DefaultDirName={localappdata}\Programs\Kuroya
DisableDirPage=yes
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
LicenseFile={#SourceRoot}\installer\LICENSE.txt
OutputDir={#SourceRoot}\dist
OutputBaseFilename=Kuroya-Setup-{#AppVersion}
SetupIconFile={#SourceRoot}\assets\logos\kuroya.ico
UninstallDisplayName=Kuroya
UninstallDisplayIcon={app}\kuroya.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
CloseApplications=yes
RestartApplications=yes
ChangesAssociations=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a Desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: checkedonce
Name: "startmenuicon"; Description: "Create a Start Menu shortcut"; GroupDescription: "Shortcuts:"; Flags: checkedonce
Name: "contextmenu"; Description: "Add ""Open with Kuroya"" to the Windows Explorer file context menu"; GroupDescription: "Windows Explorer integration:"; Flags: unchecked
Name: "associatewithfiles"; Description: "Register Kuroya as an editor for supported file types"; GroupDescription: "Windows Explorer integration:"; Flags: unchecked

[Files]
Source: "{#SourceRoot}\target\release\kuroya.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceRoot}\installer\LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Kuroya\Kuroya"; Filename: "{app}\kuroya.exe"; WorkingDir: "{app}"; IconFilename: "{app}\kuroya.exe"; Tasks: startmenuicon
Name: "{userdesktop}\Kuroya"; Filename: "{app}\kuroya.exe"; WorkingDir: "{app}"; IconFilename: "{app}\kuroya.exe"; Tasks: desktopicon

[Registry]
Root: HKA64; Subkey: "Software\Classes\*\shell\Kuroya"; ValueType: none; Flags: deletekey; Tasks: not contextmenu
Root: HKA64; Subkey: "Software\Classes\*\shell\Kuroya"; ValueType: string; ValueName: ""; ValueData: "Open with Kuroya"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKA64; Subkey: "Software\Classes\*\shell\Kuroya"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\kuroya.exe"""; Tasks: contextmenu
Root: HKA64; Subkey: "Software\Classes\*\shell\Kuroya"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Single"; Tasks: contextmenu
Root: HKA64; Subkey: "Software\Classes\*\shell\Kuroya\command"; ValueType: string; ValueName: ""; ValueData: """{app}\kuroya.exe"" ""%1"""; Tasks: contextmenu
Root: HKA64; Subkey: "Software\Classes\{#SourceFileProgId}"; ValueType: none; Flags: deletekey; Tasks: not associatewithfiles
Root: HKA64; Subkey: "Software\Classes\{#SourceFileProgId}"; ValueType: string; ValueName: ""; ValueData: "Kuroya source file"; Flags: uninsdeletekey; Tasks: associatewithfiles
Root: HKA64; Subkey: "Software\Classes\{#SourceFileProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\kuroya.exe"""; Tasks: associatewithfiles
Root: HKA64; Subkey: "Software\Classes\{#SourceFileProgId}\shell\open"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\kuroya.exe"""; Tasks: associatewithfiles
Root: HKA64; Subkey: "Software\Classes\{#SourceFileProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\kuroya.exe"" ""%1"""; Tasks: associatewithfiles
#include SourceRoot + "\target\installer\supported-file-types.iss"

[Run]
Filename: "{app}\kuroya.exe"; Description: "Launch Kuroya"; Flags: nowait postinstall skipifsilent unchecked
Filename: "{app}\kuroya.exe"; Flags: nowait skipifdoesntexist; Check: ShouldRestartKuroyaAfterUpdate

[UninstallDelete]
Type: filesandordirs; Name: "{userprograms}\Kuroya"
Type: files; Name: "{userdesktop}\Kuroya.lnk"

[Code]
function ShouldRestartKuroyaAfterUpdate: Boolean;
begin
  Result := ExpandConstant('{param:KuroyaRestart|0}') = '1';
end;
