Unicode true
ManifestSupportedOS all
RequestExecutionLevel user
SetCompressor /SOLID lzma

!define PRODUCT_NAME "RustYu Deep Scan Sparse"
!define PRODUCT_KEY "RustYuDeepScanSparse"
!define APP_DIR "$LOCALAPPDATA\Programs\${PRODUCT_NAME}"
!define UNINSTALL_DIR "$LOCALAPPDATA\RustYuSparseUninstall"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_KEY}"

Name "${PRODUCT_NAME}"
OutFile "output\RustYuDeepScanSparseSetup.exe"
InstallDir "${APP_DIR}"
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName" "Rust Yu Test Fixtures"
VIAddVersionKey "FileDescription" "Sparse legacy metadata deep-analysis fixture"
VIAddVersionKey "FileVersion" "1.0.0.0"
VIAddVersionKey "LegalCopyright" "Test fixture only"

Section "Install"
  SetRegView 64
  SetShellVarContext current

  SetOutPath "${APP_DIR}"
  File /oname=RustYuDeepScanSparse.exe "payload\common\FixtureApp.exe"
  SetOutPath "${APP_DIR}\assets\level-two"
  File /oname=sparse-resource.json "payload\sparse\legacy-state.json"

  CreateDirectory "${UNINSTALL_DIR}"
  WriteUninstaller "${UNINSTALL_DIR}\Uninstall.exe"
  SetOutPath "$LOCALAPPDATA\${PRODUCT_KEY}\State"
  File /oname=sparse-state.json "payload\sparse\legacy-state.json"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "${APP_DIR}\RustYuDeepScanSparse.exe" "--interactive"

  ; Intentionally omit InstallLocation, DisplayIcon, QuietUninstallString and size.
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "0.9.0-legacy"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Rust Yu Test Fixtures"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"${UNINSTALL_DIR}\Uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Comments" "Sparse ARP metadata fixture"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegStr HKCU "Software\RustYuFixtures\${PRODUCT_KEY}" "ActualAppRoot" "${APP_DIR}"
SectionEnd

Section "Uninstall"
  SetRegView 64
  SetShellVarContext current
  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"
  RMDir /r "$LOCALAPPDATA\${PRODUCT_KEY}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  DeleteRegKey HKCU "Software\RustYuFixtures\${PRODUCT_KEY}"
  RMDir /r "${APP_DIR}"
  RMDir /r "${UNINSTALL_DIR}"
SectionEnd
