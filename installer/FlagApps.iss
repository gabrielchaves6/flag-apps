; Inno Setup script for FlagApps.
; Builds a per-user FlagApps-Setup.exe that needs no admin rights.
; Compile with:  ISCC.exe installer\FlagApps.iss
; The flagapp.exe must already be built at target\release\flagapp.exe
; (override with /DBuildDir=... and /DAppVersion=... on the ISCC command line).

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif

[Setup]
AppId={{A3F2C1B7-8E4D-4A9F-B621-7D3E5C9A0F84}
AppName=FlagApps
AppVersion={#AppVersion}
AppPublisher=gabrielchaves6
AppPublisherURL=https://github.com/gabrielchaves6/flag-apps
DefaultDirName={autopf}\FlagApps
DefaultGroupName=FlagApps
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=dist
OutputBaseFilename=FlagApps-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=FlagApps
UninstallDisplayIcon={app}\flagapp.exe
SetupIconFile=..\crates\flagapp\assets\flag.ico
SetupLogging=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "startup"; Description: "{cm:StartAtLogon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#BuildDir}\flagapp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\crates\flagapp\assets\flag.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\FlagApps"; Filename: "{app}\flagapp.exe"; IconFilename: "{app}\flag.ico"
Name: "{userstartup}\FlagApps"; Filename: "{app}\flagapp.exe"; IconFilename: "{app}\flag.ico"; Tasks: startup

[Run]
Filename: "{app}\flagapp.exe"; Description: "{cm:LaunchProgram,FlagApps}"; Flags: nowait postinstall

[CustomMessages]
en.StartAtLogon=Start FlagApps when I sign in to Windows
brazilianportuguese.StartAtLogon=Iniciar o FlagApps ao entrar no Windows

[Code]
procedure KillRunning;
var
  ResultCode: Integer;
begin
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM flagapp.exe /F',
       '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    KillRunning;
end;

function InitializeUninstall(): Boolean;
begin
  KillRunning;
  Result := True;
end;
