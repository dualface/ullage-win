; Per-user installer for the Ullage Window app only. It does not install or
; start the ullage daemon and does not ship ullage.exe. Every path and the
; version can be overridden with ISCC /D switches.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\ullage-win.exe"
#endif
#ifndef LicenseFile
  #define LicenseFile "..\..\LICENSE"
#endif
#ifndef ReadmeFile
  #define ReadmeFile "..\..\README.md"
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

#define AppExeName "ullage-win.exe"
; Fixed by the app (src/contract.rs): the window class, the quit message and
; the Run value name. Keep them in step with that file.
#define WindowClass "UllageWindow"
#define QuitMessage "UllageWindowQuit"
#define RunValueName "Ullage Window"

[Setup]
; Not the AppId of the ullage-cli installer: the two products install side by side.
AppId={{898DD506-183F-43FF-A9A5-40085F769EAF}
AppName=Ullage Window
AppVersion={#AppVersion}
AppPublisher=dualface
AppPublisherURL=https://github.com/dualface/ullage-win
AppSupportURL=https://github.com/dualface/ullage-win/issues
DefaultDirName={localappdata}\Programs\Ullage Window
DisableProgramGroupPage=yes
LicenseFile={#LicenseFile}
OutputDir={#OutputDir}
OutputBaseFilename=ullage-win-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#AppExeName}
MinVersion=10.0
; Fallback when the quit message below does not end the app in time.
CloseApplications=yes
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "startup"; Description: "Start Ullage Window when I sign in"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
Source: "{#LicenseFile}"; DestDir: "{app}"; DestName: "LICENSE"; Flags: ignoreversion
Source: "{#ReadmeFile}"; DestDir: "{app}"; DestName: "README.md"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Ullage Window"; Filename: "{app}\{#AppExeName}"

[Registry]
; Same value the tray menu writes. The uninstaller removes it in [Code]
; whether or not this task was chosen, because the menu may have written it.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#RunValueName}"; ValueData: """{app}\{#AppExeName}"""; Tasks: startup

[Run]
; skipifsilent: a silent install (for example from a script) must not open the window.
Filename: "{app}\{#AppExeName}"; Description: "Launch Ullage Window"; Flags: nowait postinstall skipifsilent

[Code]
// Asks a running app to quit and waits up to five seconds for its window to go.
procedure QuitRunningApp;
var
  Wnd: HWND;
  Msg: Cardinal;
  Waited: Integer;
begin
  Wnd := FindWindowByClassName('{#WindowClass}');
  if Wnd = 0 then
    Exit;
  Msg := RegisterWindowMessage('{#QuitMessage}');
  PostMessage(Wnd, Msg, 0, 0);
  Waited := 0;
  while (FindWindowByClassName('{#WindowClass}') <> 0) and (Waited < 5000) do
  begin
    Sleep(100);
    Waited := Waited + 100;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  QuitRunningApp;
  Result := '';
end;

function InitializeUninstall: Boolean;
begin
  QuitRunningApp;
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', '{#RunValueName}');
end;
