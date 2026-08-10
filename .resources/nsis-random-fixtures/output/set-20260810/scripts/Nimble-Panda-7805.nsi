Target amd64-unicode
Unicode true

!include "MUI2.nsh"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_PATH}"
InstallDir "$LOCALAPPDATA\Programs\RustYuFixtures\${PRODUCT_SLUG}"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "1.0.0.${BUILD_NUMBER}"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "CompanyName" "Rust Yu Fixture Lab"
VIAddVersionKey /LANG=1033 "FileDescription" "Rust Yu randomized NSIS uninstall fixture"
VIAddVersionKey /LANG=1033 "FileVersion" "1.0.0.${BUILD_NUMBER}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Test fixture only"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install" SecInstall
  SetShellVarContext current
  SetRegView 64
  SetOutPath "$INSTDIR"
  File /r "${PAYLOAD_DIR}\*"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "1.0.0.${BUILD_NUMBER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Rust Yu Fixture Lab"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" ${ESTIMATED_SIZE_KB}
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  DeleteRegKey HKCU "${UNINSTALL_KEY}"

  ; fixture 的安装目录由生成器限定在 LocalAppData\Programs\RustYuFixtures 下。
  ; 仅删除当前产品自己的随机子目录，绝不递归清理公共父目录。
  SetOutPath "$TEMP"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR"

  ; AMD64 NSIS 偶尔会在卸载段结束后仍短暂占用自身。异步收尾脚本仅删除
  ; uninstall.exe，并仅对已经为空的产品目录执行非递归 rd。
  SetOutPath "$TEMP"
  File /oname=RustYuFixtureCleanup-${PRODUCT_SLUG}.cmd "${CLEANUP_SCRIPT}"
  Exec '"$SYSDIR\cmd.exe" /D /C "$TEMP\RustYuFixtureCleanup-${PRODUCT_SLUG}.cmd"'
SectionEnd
