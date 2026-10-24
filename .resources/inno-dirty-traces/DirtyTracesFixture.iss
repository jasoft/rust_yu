; RustYu 脏卸载夹具（dirty uninstall fixture）
;
; 目标：模拟"反安装程序留下大量残留"的遗留软件。安装包会创建完整的软件足迹
; （文件 / 注册表 / 服务 / 计划任务 / 防火墙规则 / 环境变量 / 常驻进程 / 快捷方式），
; 而它自带的卸载器（unins000.exe）运行后只清理：
;   1. 未标记 uninsneveruninstall 的文件（README.txt、config、注册脚本）
;   2. Inno 自己创建的 [Icons] 快捷方式与 ARP 卸载键
; 其余痕迹全部留下，供 Rust Yu 的残留扫描（deep analysis / leftovers）发现与清理。
;
; 注意 1：注册表残留（厂商键 / Run / App Paths / 环境变量）由 Register-ExtraTraces.ps1
; 在安装期创建——Inno 的 [Registry] 段没有"卸载时保留"的 flag（创建的值会被卸载器
; 默认删除），只有 Inno 感知不到的写入才会成为真正的残留。
; 注意 2：故意不修改系统 Path（防止破坏系统环境变量），只用应用专属变量
; RUSTYU_DIRTY_TRACES_HOME 模拟环境变量残留。
#define AppName "RustYu Dirty Traces Test App"
#define AppVersion "1.0.0"
#define AppPublisher "RustYu Fixture Labs"
#define AppIdValue "rust_yu_dirty_traces"

[Setup]
AppId={#AppIdValue}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://example.invalid/rust-yu-test-fixture
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\RustYuDirtyDaemon.exe
OutputDir=output
OutputBaseFilename=RustYuDirtyTracesSetup
Compression=lzma
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=classic
DisableProgramGroupPage=yes
SetupLogging=yes
; 常驻进程/服务运行中不需要 Restart Manager 介入，避免安装期弹窗
CloseApplications=no
; 夹具有意给"执行安装的管理员用户"留 LocalAppData 残留，此警告属预期
UsedUserAreasWarning=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
; ---- 会被卸载器删除的文件（证明卸载确实执行过） ----
Source: "payload\app\Register-ExtraTraces.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\README.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "payload\app\config\default.json"; DestDir: "{app}\config"; Flags: ignoreversion
; ---- 故意残留的文件（uninsneveruninstall） ----
Source: "payload\app\RustYuDirtyDaemon.exe"; DestDir: "{app}"; Flags: ignoreversion uninsneveruninstall
Source: "payload\app\RustYuDirtyService.exe"; DestDir: "{app}"; Flags: ignoreversion uninsneveruninstall
Source: "payload\app\logs\service-heartbeat.log"; DestDir: "{app}\logs"; Flags: ignoreversion uninsneveruninstall
Source: "payload\programdata\telemetry\pending.json"; DestDir: "{commonappdata}\RustYu Dirty Traces\telemetry"; Flags: ignoreversion uninsneveruninstall
Source: "payload\programdata\cache\index.bin"; DestDir: "{commonappdata}\RustYu Dirty Traces\cache"; Flags: ignoreversion uninsneveruninstall
Source: "payload\appdata\user-state.json"; DestDir: "{localappdata}\RustYu Dirty Traces"; DestName: "leftover-user-state.json"; Flags: ignoreversion uninsneveruninstall

[Icons]
; Inno 自己的快捷方式会被卸载器删除；真正的快捷方式残留由
; Register-ExtraTraces.ps1 以 Inno 感知不到的方式另行创建
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\RustYuDirtyDaemon.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\RustYuDirtyDaemon.exe"; Tasks: desktopicon

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
var
  ErrorCode: Integer;
  Params: string;
  UninstallKey: string;
begin
  if CurStep <> ssPostInstall then begin
    exit;
  end;

  // 注册服务 / 计划任务 / 防火墙规则 / 额外快捷方式并启动常驻进程。
  // 失败即中止安装：夹具的价值在于这些痕迹确定存在。
  Params := '-NoProfile -ExecutionPolicy Bypass -File "' + ExpandConstant('{app}') +
    '\Register-ExtraTraces.ps1" -InstallRoot "' + ExpandConstant('{app}') + '"';
  if (not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params,
      ExpandConstant('{app}'), 0, ewWaitUntilTerminated, ErrorCode)) or (ErrorCode <> 0) then
  begin
    RaiseException('Register-ExtraTraces failed (exit code ' + IntToStr(ErrorCode) + ')');
  end;

  // 补写 QuietUninstallString，静默卸载路径与 legacy 夹具保持一致
  UninstallKey := 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{#AppIdValue}_is1';
  RegWriteStringValue(HKLM, UninstallKey, 'QuietUninstallString',
    '"' + ExpandConstant('{uninstallexe}') + '" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART');
end;
