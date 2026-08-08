; Rust Yu NSIS lifecycle hooks. The maintenance entry is a fixed, elevated
; executable in $INSTDIR and receives no business arguments.
!macro NSIS_HOOK_PREUNINSTALL
  ; Ask the running GUI to close before removing its protected task.
  nsExec::ExecToLog '"$INSTDIR\rust-yu-tauri.exe" --remove-launch-tasks'
  Pop $0
  ${If} $0 != 0
    Abort "Rust Yu 无法删除管理员计划任务，卸载已中止。"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; The installer removes the Program Files payload after the pre-uninstall
  ; maintenance command has returned successfully.
!macroend
