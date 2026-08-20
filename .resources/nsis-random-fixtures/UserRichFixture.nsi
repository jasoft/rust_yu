Unicode true
ManifestSupportedOS all
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define PRODUCT_NAME "RustYu Deep Scan User"
!define PRODUCT_KEY "RustYuDeepScanUser"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_KEY}"

Name "${PRODUCT_NAME}"
OutFile "output\RustYuDeepScanUserSetup.exe"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCT_NAME}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "Rust Yu Test Fixtures"
VIAddVersionKey "FileDescription" "Deterministic current-user deep-analysis fixture"
VIAddVersionKey "FileVersion" "1.0.0.0"
VIAddVersionKey "LegalCopyright" "Test fixture only"

Section "Install"
  SetRegView 64
  SetShellVarContext current

  SetOutPath "$INSTDIR"
  File /oname=RustYuDeepScanUser.exe "payload\common\FixtureApp.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  SetOutPath "$INSTDIR\config"
  File "payload\common\config\default.json"
  SetOutPath "$INSTDIR\resources\nested"
  File /oname=user-resource.json "payload\user\user-state.json"

  SetOutPath "$APPDATA\${PRODUCT_KEY}\Profiles"
  File /oname=roaming-state.json "payload\user\user-state.json"
  SetOutPath "$LOCALAPPDATA\${PRODUCT_KEY}\Cache"
  File /oname=local-state.json "payload\user\user-state.json"
  SetOutPath "$PROFILE\AppData\LocalLow\${PRODUCT_KEY}\Telemetry"
  File /oname=locallow-state.json "payload\user\user-state.json"

  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\RustYuDeepScanUser.exe" "--interactive"
  CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\RustYuDeepScanUser.exe" "--interactive"
  FileOpen $0 "$DESKTOP\${PRODUCT_NAME}.url" w
  FileWrite $0 "[InternetShortcut]$\r$\nURL=rustyudeepscanuser://open$\r$\n"
  FileClose $0

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "1.0.0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Rust Yu Test Fixtures"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallSource" "Controlled NSIS current-user fixture"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\RustYuDeepScanUser.exe,0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://example.invalid/rust-yu-fixture"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "HelpLink" "https://example.invalid/rust-yu-fixture/help"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallDate" "20260821"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Comments" "Read-only deep-analysis truth fixture"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" 384
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1

  WriteRegStr HKCU "Software\RustYuFixtures\${PRODUCT_KEY}" "InstallRoot" "$INSTDIR"
  WriteRegStr HKCU "Software\RustYuFixtures\${PRODUCT_KEY}" "RoamingRoot" "$APPDATA\${PRODUCT_KEY}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanUser.exe" "" "$INSTDIR\RustYuDeepScanUser.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanUser.exe" "Path" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_KEY}" '"$INSTDIR\RustYuDeepScanUser.exe" --background'
  WriteRegStr HKCU "Software\Classes\rustyudeepscanuser" "" "URL:${PRODUCT_NAME} Protocol"
  WriteRegStr HKCU "Software\Classes\rustyudeepscanuser" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\rustyudeepscanuser\shell\open\command" "" '"$INSTDIR\RustYuDeepScanUser.exe" "%1"'
  WriteRegStr HKCU "Environment" "RUST_YU_DEEP_SCAN_USER_HOME" "$INSTDIR"
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext current

  RMDir /r "$SMPROGRAMS\${PRODUCT_NAME}"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.url"
  RMDir /r "$APPDATA\${PRODUCT_KEY}"
  RMDir /r "$LOCALAPPDATA\${PRODUCT_KEY}"
  RMDir /r "$PROFILE\AppData\LocalLow\${PRODUCT_KEY}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\RustYuFixtures\${PRODUCT_KEY}"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\App Paths\RustYuDeepScanUser.exe"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT_KEY}"
  DeleteRegKey HKCU "Software\Classes\rustyudeepscanuser"
  DeleteRegValue HKCU "Environment" "RUST_YU_DEEP_SCAN_USER_HOME"
  RMDir /r "$INSTDIR"
SectionEnd
