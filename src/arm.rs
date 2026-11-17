//! Running ARM-only Android apps on x86 Android.
//!
//! Android-x86 cannot execute ARM machine code by itself. Two well known
//! translators make it possible:
//!
//! * **libhoudini** (Intel's ARM translator, used by Android-x86 / BlissOS) -
//!   activated inside the guest by the `enable_nativebridge` helper or by
//!   installing the `houdini.sfs` / `libhoudini` payload into `/system`.
//! * **libndk** (Google's NDK translation, BlissOS 12+) - shipped as a magisk
//!   module or an `libndk_translation` folder in `/system`.
//!
//! This module detects ARM-only APKs, reports whether the guest can translate
//! ARM, and performs the in-guest activation through `adb`.

use crate::apk::ApkInfo;
use crate::qemu::AdbClient;

/// Does this APK contain ARM code and no x86 code?
pub fn is_arm_only(info: &ApkInfo) -> bool {
    if info.abis.is_empty() {
        return false;
    }
    let has_arm = info
        .abis
        .iter()
        .any(|a| a.starts_with("arm") || a == "armeabi");
    let has_x86 = info.abis.iter().any(|a| a.starts_with("x86"));
    has_arm && !has_x86
}

/// Kotlin/Java-only APKs (no native libraries) run fine anywhere.
pub fn needs_translation(info: &ApkInfo) -> bool {
    is_arm_only(info)
}

/// Short description shown in the UI.
pub fn describe_apk(info: &ApkInfo) -> String {
    if info.abis.is_empty() {
        return "Чистый Java/Kotlin — работает на любом образе.".to_string();
    }
    if is_arm_only(info) {
        return format!(
            "ARM-only приложение ({}). Нужна ARM-трансляция: libhoudini/libndk \
             в образе (BlissOS/PrimeOS) или включённая опция «Поддержка ARM».",
            info.abis.join(", ")
        );
    }
    format!(
        "Есть x86-код ({}). Запустится напрямую.",
        info.abis.join(", ")
    )
}

/// Text for the checkbox in the settings page.
pub fn status_summary(enabled: bool) -> String {
    if enabled {
        "Включено: при запуске эмулятор проверит ARM-трансляцию в госте и, если её нет, \
         попробует активировать libhoudini (enable_nativebridge) или подскажет, \
         какой компонент добавить в образ."
            .to_string()
    } else {
        "Выключено: ARM-only приложения (большинство игр из Play Store) не запустятся, \
         если в образе нет libhoudini/libndk."
            .to_string()
    }
}

/// Extra diagnostic line for the Doctor page.
pub fn guest_support_hint() -> Option<String> {
    Some(
        "ARM: для ARM-приложений используйте образ с libhoudini/libndk (BlissOS, PrimeOS) \
         или включите опцию «Поддержка ARM» — она активирует трансляцию в госте."
            .to_string(),
    )
}

#[derive(Debug, Clone)]
pub struct ArmStatus {
    /// `ro.product.cpu.abilist` from the guest.
    pub abilist: String,
    /// True when houdini/libndk is present and usable.
    pub translation_ready: bool,
    /// Which translator was detected.
    pub translator: Option<String>,
}

impl ArmStatus {
    pub fn summary(&self) -> String {
        match &self.translator {
            Some(name) if self.translation_ready => {
                format!("ARM-трансляция активна ({name}). abilist: {}", self.abilist)
            }
            _ => format!(
                "ARM-трансляция не найдена. abilist: {}",
                if self.abilist.is_empty() {
                    "(гость не ответил)"
                } else {
                    &self.abilist
                }
            ),
        }
    }
}

/// Ask the guest which ABIs it supports and whether a translator is present.
pub fn probe(adb: &AdbClient) -> ArmStatus {
    let abilist = adb
        .shell("getprop ro.product.cpu.abilist")
        .unwrap_or_default()
        .trim()
        .to_string();
    let abi = adb
        .shell("getprop ro.product.cpu.abi")
        .unwrap_or_default()
        .trim()
        .to_string();
    let combined = format!("{abilist},{abi}");

    // libhoudini exposes arm ABIs through the abilist; libndk adds
    // `libndk_translation`. Check both the property and the filesystem.
    let houdini_file = adb
        .shell("ls /system/lib/libhoudini.so /system/lib64/libhoudini.so 2>/dev/null")
        .unwrap_or_default();
    let ndk_file = adb
        .shell("ls /system/lib/libndk_translation.so /system/lib64/libndk_translation.so 2>/dev/null")
        .unwrap_or_default();

    let translator = if combined.contains("arm") || combined.contains("houdini") {
        Some("libhoudini".to_string())
    } else if houdini_file.contains("libhoudini") {
        Some("libhoudini (не активирован)".to_string())
    } else if ndk_file.contains("ndk_translation") {
        Some("libndk".to_string())
    } else {
        None
    };
    let translation_ready = combined.contains("armeabi")
        || combined.contains("arm64")
        || ndk_file.contains("ndk_translation");

    ArmStatus {
        abilist,
        translation_ready,
        translator,
    }
}

/// Try to switch ARM translation on inside the running guest.
///
/// Android-x86 images based on BlissOS/PrimeOS ship `enable_nativebridge`
/// (houdini) and/or a libndk magisk module. Both are root operations; if the
/// guest is not rooted this returns a helpful error instead of failing silently.
pub fn enable_in_guest(adb: &AdbClient) -> Result<String, String> {
    let mut log = Vec::new();
    let status = probe(adb);
    if status.translation_ready {
        log.push(status.summary());
        return Ok(log.join("\n"));
    }

    // 1) houdini through the vendor helper.
    let native_bridge = adb
        .shell("ls /system/bin/enable_nativebridge /system/xbin/enable_nativebridge 2>/dev/null")
        .unwrap_or_default();
    if native_bridge.contains("enable_nativebridge") {
        let out = adb
            .shell("su -c 'enable_nativebridge' 2>&1 || enable_nativebridge 2>&1")
            .unwrap_or_default();
        log.push(format!("enable_nativebridge: {}", out.trim()));
        let after = probe(adb);
        if after.translation_ready {
            log.push(after.summary());
            return Ok(log.join("\n"));
        }
    }

    // 2) libndk magisk module already unpacked in /system.
    let ndk_present = adb
        .shell("ls /system/lib/libndk_translation.so 2>/dev/null")
        .unwrap_or_default();
    if ndk_present.contains("libndk_translation") {
        let out = adb
            .shell("su -c 'setprop persist.sys.nativebridge 1; setprop ro.dalvik.vm.native.bridge libndk_translation.so' 2>&1")
            .unwrap_or_default();
        log.push(format!("libndk: {}", out.trim()));
        let after = probe(adb);
        if after.translation_ready {
            log.push(after.summary());
            return Ok(log.join("\n"));
        }
    }

    Err(format!(
        "{} \n\n\
         В этом образе нет готовой ARM-трансляции. Что делать:\n\
         1) Использовать образ BlissOS / PrimeOS — там libhoudini уже включён.\n\
         2) Или установить libndk/libhoudini в гостя вручную (нужен root в образе).\n\
         3) Или взять x86-сборку приложения (если разработчик её выпускает).",
        if log.is_empty() {
            "Трансляция не найдена.".to_string()
        } else {
            log.join("\n")
        }
    ))
}

/// Install a libndk / libhoudini payload (a Magisk module zip) into the guest.
///
/// This is the supported way to add ARM translation to an image that lacks it:
/// the user downloads a module zip (for example `libndk_translation` for
/// BlissOS) and the launcher pushes it to the guest and installs it with
/// `magisk --install-module`.
pub fn install_payload(adb: &AdbClient, module_zip: &std::path::Path) -> Result<String, String> {
    if !module_zip.is_file() {
        return Err(format!("{} not found", module_zip.display()));
    }
    let name = module_zip
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "module.zip".to_string());
    let remote = format!("/data/local/tmp/{name}");
    let output = adb.push(module_zip, &remote)?;
    let mut log = vec![format!("uploaded: {output}")];

    let magisk = adb
        .shell("which magisk 2>/dev/null || ls /sbin/magisk 2>/dev/null")
        .map_err(|e| e.to_string())?;
    if magisk.trim().is_empty() {
        return Err(
            "В госте нет Magisk: установить модуль нельзя.\n\
             Используйте образ BlissOS (houdini уже включён) или образ с root/Magisk."
                .to_string(),
        );
    }
    let out = adb
        .shell(&format!("su -c 'magisk --install-module {remote}' 2>&1"))
        .map_err(|e| e.to_string())?;
    log.push(out.trim().to_string());
    if out.to_lowercase().contains("error") || out.to_lowercase().contains("failed") {
        return Err(log.join("\n"));
    }
    let _ = adb.shell("su -c 'reboot' 2>&1");
    log.push("Гость перезагружается, чтобы активировать модуль.".to_string());
    Ok(log.join("\n"))
}

/// Called before launching an ARM-only app with the option enabled.
pub fn prepare_for_app(adb_port: u16, app: &ApkInfo, enabled: bool) -> Option<String> {
    if !enabled || !needs_translation(app) {
        return None;
    }
    let Ok(adb) = AdbClient::detect(adb_port) else {
        return Some("adb не найден: ARM-трансляцию проверить не удалось.".to_string());
    };
    match enable_in_guest(&adb) {
        Ok(message) => Some(message),
        Err(message) => Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apk::ApkInfo;

    fn info(abis: &[&str]) -> ApkInfo {
        ApkInfo {
            abis: abis.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn detects_arm_only_packages() {
        assert!(is_arm_only(&info(&["arm64-v8a", "armeabi-v7a"])));
        assert!(!is_arm_only(&info(&["arm64-v8a", "x86_64"])));
        assert!(!is_arm_only(&info(&["x86"])));
        assert!(!is_arm_only(&info(&[])));
    }

    #[test]
    fn describes_packages() {
        assert!(describe_apk(&info(&[])).contains("Java"));
        assert!(describe_apk(&info(&["arm64-v8a"])).contains("ARM-only"));
        assert!(describe_apk(&info(&["x86_64"])).contains("x86"));
    }
}
