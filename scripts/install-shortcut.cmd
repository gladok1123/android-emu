@echo off
rem Registers .apk -> android-emu and prints diagnostics.
setlocal
cd /d "%~dp0.."
"%~dp0..\android-emu.exe" setup
"%~dp0..\android-emu.exe" doctor
pause
endlocal
