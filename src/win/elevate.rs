//! Running things with administrator rights.
//!
//! Used for the "enable Windows Hypervisor Platform" button: a small script is
//! written to the data folder and started through `ShellExecuteW` with the
//! `runas` verb, which shows the UAC prompt.

use super::ffi::*;
use std::path::{Path, PathBuf};

const SW_SHOWNORMAL: i32 = 1;

/// Start `file` with the given parameters, asking for elevation.
pub fn shell_execute_runas(file: &Path, params: &str) -> Result<(), String> {
    unsafe {
        let verb = to_wide("runas");
        let file_w = to_wide(&file.to_string_lossy());
        let params_w = to_wide(params);
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file_w.as_ptr(),
            params_w.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        // ShellExecuteW returns a value > 32 on success.
        if (result as isize) <= 32 {
            let code = result as isize;
            let message = match code {
                5 => "отказано в доступе (UAC отменён)".to_string(),
                2 => "файл не найден".to_string(),
                31 => "нет ассоциированного приложения".to_string(),
                other => format!("код ошибки ShellExecuteW: {other}"),
            };
            return Err(message);
        }
    }
    Ok(())
}

/// Write (and return the path of) the script that enables WHPX.
pub fn write_whpx_script() -> std::io::Result<PathBuf> {
    crate::paths::ensure_data_dir()?;
    let path = crate::paths::data_dir().join("enable-hypervisor-platform.cmd");
    let script = r#"@echo off
chcp 65001 >nul
title android-emu: Windows Hypervisor Platform
echo.
echo === android-emu: включаем Windows Hypervisor Platform (WHPX) ===
echo.
echo Этот скрипт включает компоненты Windows, нужные QEMU для аппаратного
echo ускорения Android. Требуются права администратора и перезагрузка.
echo.

dism /online /enable-feature /featurename:HypervisorPlatform /all /norestart
if errorlevel 1 (
  echo.
  echo [1/2] HypervisorPlatform: не удалось (возможно, уже включено).
) else (
  echo [1/2] HypervisorPlatform: включено.
)

dism /online /enable-feature /featurename:Microsoft-Hyper-V-Hypervisor /all /norestart
if errorlevel 1 (
  echo [2/2] Hyper-V Hypervisor: пропущено или уже включено.
) else (
  echo [2/2] Hyper-V Hypervisor: включено.
)

echo.
echo Проверка состояния:
dism /online /get-featureinfo /featurename:HypervisorPlatform | findstr /i "State"
echo.
echo Готово. Перезагрузите компьютер, затем запустите android-emu doctor.
echo.
pause
"#;
    std::fs::write(&path, script)?;
    Ok(path)
}

/// Write and run the WHPX script elevated. Returns the script path.
pub fn enable_hypervisor_platform() -> Result<PathBuf, String> {
    let script = write_whpx_script().map_err(|e| e.to_string())?;
    // `cmd /c` keeps the window open through `pause` inside the script.
    let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    let params = format!("/c \"\"{}\"\"", script.display());
    shell_execute_runas(Path::new(&comspec), &params)?;
    Ok(script)
}

/// Relaunch `android-emu` itself elevated (used by `--elevate` helpers).
pub fn relaunch_elevated(args: &[String]) -> Result<(), String> {
    let exe = super::shortcut::exe_path();
    let params = args
        .iter()
        .map(|a| crate::util::win_quote(a))
        .collect::<Vec<_>>()
        .join(" ");
    shell_execute_runas(&exe, &params)
}
