#define AppName "RustYu Legacy Test App"
#define AppVersion "1.0.0"
#define AppPublisher "rust_yu Test Fixtures"
#define AppIdValue "rust_yu_legacy_test_app"
#define AppExeName "LegacyLauncher.cmd"
[Setup]
AppId={#AppIdValue}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://example.invalid/rust-yu-test-fixture
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayName={#AppName}
UninstallDisplayIcon={sys}\cmd.exe
OutputDir=output
OutputBaseFilename=RustYuLegacyTestSetup
Compression=lzma
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=classic
DisableProgramGroupPage=yes
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "payload\app\LegacyLauncher.cmd"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\SpawnUninstallHelper.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\UninstallWorker.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\config\default.json"; DestDir: "{app}\config"; Flags: ignoreversion
Source: "payload\app\logs\leftover.log"; DestDir: "{app}\logs"; Flags: ignoreversion uninsneveruninstall
Source: "payload\appdata\user-profile.json"; DestDir: "{localappdata}\RustYuLegacyTest\Data"; DestName: "leftover-user-profile.json"; Flags: ignoreversion uninsneveruninstall

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch the legacy test fixture"; Flags: postinstall skipifsilent nowait

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var
  UninstallKey: string;
  InteractiveCommand: string;
  QuietCommand: string;
begin
  if CurStep <> ssPostInstall then begin
    exit;
  end;

  UninstallKey := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppIdValue}_is1';
  InteractiveCommand :=
    '"' + ExpandConstant('{app}\SpawnUninstallHelper.exe') + '" interactive';
  QuietCommand :=
    '"' + ExpandConstant('{app}\SpawnUninstallHelper.exe') + '" quiet';

  RegWriteStringValue(HKLM, UninstallKey, 'UninstallString', InteractiveCommand);
  RegWriteStringValue(HKLM, UninstallKey, 'QuietUninstallString', QuietCommand);
end;
