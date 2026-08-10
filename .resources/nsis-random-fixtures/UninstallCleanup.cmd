@echo off
setlocal
set "fixture_dir=%LOCALAPPDATA%\Programs\RustYuFixtures\__PRODUCT_SLUG__"

rem NSIS 的 AMD64 uninstaller 仍可能短暂占用自身；等待进程退出后重试。
rem 这里只删除固定文件名，并使用非递归 rd。即使收到异常路径也不会递归删除用户数据。
for /L %%I in (1,1,100) do (
  del /F /Q "%fixture_dir%\uninstall.exe" >nul 2>nul
  rd "%fixture_dir%" >nul 2>nul && goto :done
  ping 127.0.0.1 -n 2 >nul
)

:done
endlocal
del /F /Q "%~f0" >nul 2>nul
