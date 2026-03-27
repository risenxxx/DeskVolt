; DeskVolt Inno Setup Script
; Installer for DeskVolt - Battery Widget for Wireless Peripherals

#define MyAppName "DeskVolt"
#define MyAppPublisher "DeskVolt Contributors"
#define MyAppURL "https://github.com/risenxxx/deskvolt"
#define MyAppExeName "deskvolt.exe"

[Setup]
; Unique AppId - DO NOT CHANGE after first release
AppId={{D8E5F7A2-B1C3-4D6E-A9F0-123456789ABC}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}/releases
; Default to Program Files, but user can change
DefaultDirName={autopf}\{#MyAppName}
; Allow user to change installation directory
DisableDirPage=no
DefaultGroupName={#MyAppName}
; No license page
LicenseFile=
; Allow user to skip Start Menu folder
AllowNoIcons=yes
; Output settings
OutputDir=..\target\installer
OutputBaseFilename=deskvolt-setup-{#MyAppVersion}
; Modern visual style
WizardStyle=modern
; Compression
Compression=lzma2
SolidCompression=yes
; No admin required - installs to user's local app data by default
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; 64-bit only
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Uninstall settings
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
; Allows upgrading without uninstalling
UsePreviousAppDir=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[CustomMessages]
english.RunAtStartup=Run at Windows startup
english.StartupGroup=Startup:
russian.RunAtStartup=Запускать при входе в Windows
russian.StartupGroup=Автозапуск:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startup"; Description: "{cm:RunAtStartup}"; GroupDescription: "{cm:StartupGroup}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
; Create startup registry entry if selected
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Registry]
; Add to startup via registry (user-level, no admin required)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "DeskVolt"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: startup

[Code]
// Close running instance before install/uninstall
function InitializeSetup(): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  // Try to close running instance gracefully
  Exec('taskkill', '/f /im deskvolt.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function InitializeUninstall(): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  // Close running instance before uninstall
  Exec('taskkill', '/f /im deskvolt.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

// Clean up config file on uninstall
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ConfigFile: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    ConfigFile := ExpandConstant('{app}\deskvolt.ini');
    DeleteFile(ConfigFile);
  end;
end;
