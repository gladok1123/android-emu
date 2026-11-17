@echo off
rem Build android-emu for Windows (64-bit).
setlocal
cd /d "%~dp0.."

where cargo >nul 2>nul
if errorlevel 1 (
  echo Cargo not found. Install Rust from https://rustup.rs and run this again.
  exit /b 1
)

echo Building release binary...
cargo build --release
if errorlevel 1 (
  echo Build failed.
  exit /b 1
)

copy /y "target\release\android-emu.exe" "android-emu.exe" >nul
echo.
echo Done: %CD%\android-emu.exe
endlocal
