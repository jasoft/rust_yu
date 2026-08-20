Unicode true
ManifestSupportedOS all
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define PRODUCT_NAME "RustYu Deep Scan Machine"
!define PRODUCT_KEY "RustYuDeepScanMachine"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_KEY}"

Name "${PRODUCT_NAME}"
OutFile "output\RustYuDeepScanMachineSetup.exe"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
InstallDirRegKey HKLM "${UNINSTALL_KEY}" "InstallLocation"
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "Rust Yu Test Fixtures"
VIAddVersionKey "FileDescription" "Deterministic machine-scope deep-analysis fixture"
VIAddVersionKey "FileVersion" "1.0.0.0"
VIAddVersionKey "LegalCopyright" "Test fixture only"

Section "Install"
  SetRegView 64
  SetShellVarContext all

  SetOutPath "$INSTDIR"
  File /oname=RustYuDeepScanMachine.exe "payload\common\FixtureApp.exe"
  File /oname=fixture-shared.dll "payload\common\FixtureApp.exe"
  File "payload\machine\Register-MachineIntegrations.ps1"
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  SetOutPath "$INSTDIR\config"
  File "payload\common\config\default.json"
  SetOutPath "$INSTDIR\resources\nested"
  File /oname=machine-resource.json "payload\machine\machine-state.json"

  SetOutPath "$APPDATA\RustYuDeepScanMachine\Logs"
  File /oname=machine-state.json "payload\machine\machine-state.json"

  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\RustYuDeepScanMachine.exe" "--interactive"
  CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\RustYuDeepScanMachine.exe" "--interactive"

  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "1.0.0"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Publisher" "Rust Yu Test Fixtures"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallSource" "Controlled NSIS machine fixture"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\RustYuDeepScanMachine.exe,0"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr HKLM "${UNINSTALL_KEY}" "URLInfoAbout" "https://example.invalid/rust-yu-fixture"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "HelpLink" "https://example.invalid/rust-yu-fixture/help"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "InstallDate" "20260821"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "Comments" "Read-only deep-analysis truth fixture"
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "EstimatedSize" 512
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1

  WriteRegStr HKLM "Software\RustYuFixtures\${PRODUCT_KEY}" "InstallRoot" "$INSTDIR"
  WriteRegStr HKLM "Software\RustYuFixtures\${PRODUCT_KEY}" "DataRoot" "$APPDATA\${PRODUCT_KEY}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanMachine.exe" "" "$INSTDIR\RustYuDeepScanMachine.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanMachine.exe" "Path" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_KEY}" '"$INSTDIR\RustYuDeepScanMachine.exe" --background'
  WriteRegStr HKLM "Software\Classes\rustyudeepscanmachine" "" "URL:${PRODUCT_NAME} Protocol"
  WriteRegStr HKLM "Software\Classes\rustyudeepscanmachine" "URL Protocol" ""
  WriteRegStr HKLM "Software\Classes\rustyudeepscanmachine\shell\open\command" "" '"$INSTDIR\RustYuDeepScanMachine.exe" "%1"'
  WriteRegStr HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "RUST_YU_DEEP_SCAN_MACHINE_HOME" "$INSTDIR"
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\SharedDLLs" "$INSTDIR\fixture-shared.dll" 1
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Installer\Folders" "$INSTDIR\" ""

  ExecWait '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\Register-MachineIntegrations.ps1" -ExecutablePath "$INSTDIR\RustYuDeepScanMachine.exe"' $0
  IntCmp $0 0 integrations_ready
  Abort "Failed to create controlled service and scheduled-task fixtures (exit $0)"
integrations_ready:
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext all

  ExecWait '"$SYSDIR\schtasks.exe" /Delete /TN "RustYu Deep Scan Machine Fixture" /F'
  ExecWait '"$SYSDIR\sc.exe" delete RustYuDeepScanMachine'

  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  RMDir /r "$SMPROGRAMS\${PRODUCT_NAME}"
  RMDir /r "$APPDATA\${PRODUCT_KEY}"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
  DeleteRegKey HKLM "Software\RustYuFixtures\${PRODUCT_KEY}"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanMachine.exe"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_KEY}"
  DeleteRegKey HKLM "Software\Classes\rustyudeepscanmachine"
  DeleteRegValue HKLM "SYSTEM\CurrentControlSet\Control\Session Manager\Environment" "RUST_YU_DEEP_SCAN_MACHINE_HOME"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\SharedDLLs" "$INSTDIR\fixture-shared.dll"
  DeleteRegValue HKLM "Software\Microsoft\Windows\CurrentVersion\Installer\Folders" "$INSTDIR\"
  RMDir /r "$INSTDIR"
SectionEnd
