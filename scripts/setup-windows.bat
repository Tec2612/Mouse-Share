@echo off
REM Double-click this file to run the automated setup. It exists because
REM Windows opens .ps1 files in Notepad by default when double-clicked
REM rather than running them, and because a fresh machine's execution
REM policy normally blocks unsigned scripts outright.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-windows.ps1"
echo.
pause
