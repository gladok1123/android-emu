//! QEMU backend: locating the binary, building the command line, launching a
//! session and talking to the guest through `adb`.

use crate::store::{AppEntry, DisplayMode, GpuMode};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum QemuError {
    NotFound(String),
    Launch(String),
    AdbMissing,
    Adb(String),
    NoImage,
}

impl fmt::Display for QemuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QemuError::NotFound(m) => write!(f, "QEMU was not found: {m}"),
            QemuError::Launch(m) => write!(f, "could not start QEMU: {m}"),
            QemuError::AdbMissing => {
                write!(f, "adb was not found - install platform-tools or run `android-emu setup`")
            }
            QemuError::Adb(m) => write!(f, "adb error: {m}"),
            QemuError::NoImage => write!(f, "no Android image installed (run `android-emu image add <iso>`)"),
        }
    }
}

impl std::error::Error for QemuError {}

/// Result of probing the system for QEMU.
#[derive(Debug, Clone)]
pub struct QemuInstall {
    pub exe: PathBuf,
    pub version: String,
    pub source: QemuSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QemuSource {
    /// `qemu-system-x86_64.exe` found on PATH.
    Path,
    /// Found in `%LOCALAPPDATA%\android-emu\qemu`.
    Managed,
    /// Configured explicitly in settings.
    Configured,
}

const QEMU_EXE: &str = "qemu-system-x86_64";

/// Look for QEMU in the managed folder, then on PATH.
pub fn detect() -> Option<QemuInstall> {
    if let Some(configured) = crate::config::configured_qemu_path() {
        if configured.is_file() {
            return Some(QemuInstall {
                version: probe_version(&configured),
                exe: configured,
                source: QemuSource::Configured,
            });
        }
    }
    let managed = crate::paths::qemu_dir().join(format!("{QEMU_EXE}.exe"));
    if managed.is_file() {
        return Some(QemuInstall {
            version: probe_version(&managed),
            exe: managed,
            source: QemuSource::Managed,
        });
    }
    // Some distributions keep the binary in a subfolder.
    let managed_alt = crate::paths::qemu_dir().join("qemu").join(format!("{QEMU_EXE}.exe"));
    if managed_alt.is_file() {
        return Some(QemuInstall {
            version: probe_version(&managed_alt),
            exe: managed_alt,
            source: QemuSource::Managed,
        });
    }
    if let Some(found) = crate::util::which(QEMU_EXE) {
        return Some(QemuInstall {
            version: probe_version(&found),
            exe: found,
            source: QemuSource::Path,
        });
    }
    // A few portable builds use `qemu-system-x86_64w.exe`.
    if let Some(found) = crate::util::which("qemu-system-x86_64w") {
        return Some(QemuInstall {
            version: probe_version(&found),
            exe: found,
            source: QemuSource::Path,
        });
    }
    None
}

fn probe_version(exe: &Path) -> String {
    Command::new(exe)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            text.lines().next().map(|l| l.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Options for a QEMU session.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub image: PathBuf,
    pub memory_mb: u32,
    pub cores: u32,
    pub resolution: (u32, u32),
    pub dpi: u32,
    pub gpu: GpuMode,
    pub display_mode: DisplayMode,
    pub window_size: (u32, u32),
    pub adb_port: u16,
    pub extra_args: Vec<String>,
    pub snapshot: bool,
    /// Enable emulated audio (disable when QEMU fails to start on a headless box).
    pub audio: bool,
    /// Name of the per-app session config used when the VM is shared.
    pub session_name: String,
}

impl SessionOptions {
    /// Screen size actually emulated: the app window when the app is not
    /// fullscreen, otherwise the configured resolution.
    pub fn screen_px(&self) -> (u32, u32) {
        if self.display_mode == DisplayMode::Fullscreen {
            self.resolution
        } else {
            let (w, h) = self.window_size;
            if w >= 320 && h >= 320 {
                (w, h)
            } else {
                self.resolution
            }
        }
    }

    pub fn from_entry(entry: &AppEntry, image: PathBuf, adb_port: u16) -> SessionOptions {
        let (rw, rh) = entry.resolution_px();
        SessionOptions {
            image,
            memory_mb: 2048,
            cores: 2,
            resolution: (rw, rh),
            dpi: if entry.dpi == 0 { 240 } else { entry.dpi },
            gpu: entry.gpu,
            display_mode: entry.display_mode,
            window_size: (entry.window_width, entry.window_height),
            adb_port,
            extra_args: entry.extra_args.clone(),
            snapshot: false,
            audio: true,
            session_name: entry.slug.clone(),
        }
    }

    /// Build the full QEMU argument list.
    pub fn build_args(&self, accel: Accel) -> Vec<String> {
        let mut args: Vec<String> = Vec::new();
        let (rw, rh) = self.screen_px();
        let (ww, wh) = self.window_size;

        // Machine + acceleration.
        args.push("-machine".into());
        args.push("q35,usb=on".into());
        match accel {
            Accel::Whpx => args.extend(["-accel".into(), "whpx,ignored=unimplemented".into()]),
            Accel::Haxm => args.extend(["-accel".into(), "hax".into()]),
            Accel::Tcg => args.extend(["-accel".into(), "tcg,thread=multi".into()]),
        }
        // CPU model: hardware virtualization passes the host CPU through, TCG
        // needs a superset model or some Android builds refuse to boot.
        args.extend(["-cpu".into(), match accel {
            Accel::Whpx | Accel::Haxm => "host".to_string(),
            Accel::Tcg => "max".to_string(),
        }]);
        args.extend(["-smp".into(), self.cores.to_string()]);
        args.extend(["-m".into(), self.memory_mb.to_string()]);
        args.extend(["-rtc".into(), "base=utc".to_string()]);

        // Disk: the Android-x86 image, with an optional persistent overlay.
        let mut disk = format!(
            "file={},format=raw,if=virtio,cache=writeback,discard=unmap",
            self.image.display()
        );
        if self.snapshot {
            // `snapshot=on` keeps the base image clean; per-app data persistence is
            // handled by the overlay created by `image create-overlay`.
            let overlay = crate::paths::images_dir()
                .join("overlays")
                .join(format!("{}.qcow2", safe_overlay_name(&self.session_name)));
            if overlay.is_file() {
                disk = format!(
                    "file={},format=qcow2,if=virtio,cache=writeback,discard=unmap",
                    overlay.display()
                );
            } else {
                disk.push_str(",snapshot=on");
            }
        }
        args.extend(["-drive".into(), disk]);

        // Networking: user-mode NIC with adb forwarding.
        args.extend([
            "-netdev".into(),
            format!(
                "user,id=net0,hostfwd=tcp:127.0.0.1:{}-:5555",
                self.adb_port
            ),
            "-device".into(),
            "virtio-net-pci,netdev=net0".into(),
        ]);

        // Graphics. Android-x86 renders in software by default; `-vga std` is the
        // mode every build supports. GPU-related tuning is opt-in through
        // `--extra-arg` so the defaults never break booting.
        match self.display_mode {
            DisplayMode::Headless => {
                args.extend(["-display".into(), "none".into()]);
            }
            DisplayMode::Fullscreen => {
                args.extend(["-display".into(), String::from("sdl,show-cursor=off")]);
                args.extend(["-full-screen".into()]);
            }
            DisplayMode::Window => {
                args.extend(["-display".into(), String::from("sdl,show-cursor=on")]);
            }
        }
        // One graphics adapter only. The EDID block tells Android-x86 which
        // resolution to use, which is what makes the window size meaningful.
        match self.gpu {
            GpuMode::Angle => args.extend(["-device".into(), "virtio-vga".to_string()]),
            _ if self.display_mode == DisplayMode::Headless => {
                args.extend(["-vga".into(), "std".to_string()])
            }
            _ => args.extend([
                "-device".into(),
                format!("VGA,edid=on,xres={},yres={}", rw, rh),
            ]),
        }

        // Input devices: touch + keyboard.
        args.extend([
            "-device".into(),
            "usb-tablet,bus=usb-bus.0".into(),
            "-device".into(),
            "usb-kbd,bus=usb-bus.0".into(),
        ]);

        // Audio (can be disabled per app with --no-audio, useful on machines
        // without a sound device where QEMU would refuse to start).
        if self.audio {
            #[cfg(windows)]
            let backend = "dsound,id=snd0";
            #[cfg(not(windows))]
            let backend = "none,id=snd0";
            args.extend(["-audiodev".into(), backend.to_string()]);
            args.extend(["-device".into(), "intel-hda".into()]);
            args.extend(["-device".into(), "hda-duplex,audiodev=snd0".into()]);
        }

        let _ = (ww, wh);

        // Boot: no menu, straight to the disk.
        args.extend(["-boot".into(), "order=c,menu=off".into()]);
        args.extend(["-no-reboot".into()]);

        // Monitor on a private loopback socket so the shell can shut the guest
        // down cleanly. Ports are offset from the adb port (allocated in 4s).
        args.extend([
            "-monitor".into(),
            format!(
                "tcp:127.0.0.1:{},server=on,wait=off",
                self.adb_port.saturating_add(1)
            ),
        ]);
        args.extend(["-name".into(), format!("android-emu: {}", self.session_name)]);

        args.extend(self.extra_args.iter().cloned());
        args
    }
}

fn safe_overlay_name(name: &str) -> String {
    crate::util::sanitize_file_name(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    Whpx,
    Haxm,
    Tcg,
}

impl Accel {
    pub fn label(self) -> &'static str {
        match self {
            Accel::Whpx => "Windows Hypervisor Platform",
            Accel::Haxm => "Intel HAXM",
            Accel::Tcg => "software emulation (slow)",
        }
    }
}

/// Decide which acceleration to use.
pub fn detect_accel() -> Accel {
    if std::env::var_os("ANDROID_EMU_FORCE_TCG").is_some() {
        return Accel::Tcg;
    }
    #[cfg(windows)]
    {
        // WHPX ships with Windows 10/11 when the feature is enabled; probing the
        // registry is cheap and avoids a failed QEMU start.
        if crate::win::is_whpx_enabled() {
            return Accel::Whpx;
        }
        if crate::win::is_haxm_installed() {
            return Accel::Haxm;
        }
    }
    Accel::Tcg
}

/// ADB client bound to a QEMU session's forwarded port.
pub struct AdbClient {
    pub adb: PathBuf,
    pub port: u16,
}

impl AdbClient {
    pub fn detect(port: u16) -> Result<AdbClient, QemuError> {
        let adb = find_adb().ok_or(QemuError::AdbMissing)?;
        Ok(AdbClient { adb, port })
    }

    fn base_cmd(&self) -> Command {
        let mut cmd = Command::new(&self.adb);
        cmd.arg("-P").arg(self.port.to_string());
        cmd
    }

    /// `adb connect` to the forwarded port. Returns true when already connected.
    pub fn connect(&self) -> Result<(), QemuError> {
        let out = self
            .base_cmd()
            .arg("connect")
            .arg(format!("127.0.0.1:{}", self.port))
            .output()
            .map_err(|e| QemuError::Adb(e.to_string()))?;
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        if text.contains("connected") || text.contains("already") {
            Ok(())
        } else {
            Err(QemuError::Adb(String::from_utf8_lossy(&out.stdout).trim().to_string()))
        }
    }

    /// Run a shell command on the device.
    pub fn shell(&self, command: &str) -> Result<String, QemuError> {
        let out = self
            .base_cmd()
            .args(["shell", command])
            .output()
            .map_err(|e| QemuError::Adb(e.to_string()))?;
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            if !err.trim().is_empty() {
                text.push_str(&err);
            }
        }
        Ok(text)
    }

    /// Wait until the device answers `getprop sys.boot_completed`.
    pub fn wait_for_boot(&self, timeout: Duration, mut progress: impl FnMut(&str)) -> bool {
        let start = Instant::now();
        let mut last_status = String::new();
        while start.elapsed() < timeout {
            if let Ok(out) = self.shell("getprop sys.boot_completed") {
                let status = out.trim().to_string();
                if status == "1" {
                    progress("boot completed");
                    return true;
                }
                if status != last_status {
                    progress(&format!("boot_completed={status:?}"));
                    last_status = status;
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        false
    }

    pub fn install(&self, apk: &Path) -> Result<String, QemuError> {
        let out = self
            .base_cmd()
            .arg("install")
            .arg("-r")
            .arg("-g")
            .arg(apk)
            .output()
            .map_err(|e| QemuError::Adb(e.to_string()))?;
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        if out.status.success() && text.contains("Success") {
            Ok(text)
        } else {
            Err(QemuError::Adb(text.trim().to_string()))
        }
    }

    pub fn start_activity(&self, component: &str) -> Result<String, QemuError> {
        self.shell(&format!(
            "am start -W -n {} -a android.intent.action.MAIN -c android.intent.category.LAUNCHER",
            component
        ))
    }

    /// Simple check that the device is reachable.
    pub fn is_online(&self) -> bool {
        self.shell("echo ok").map(|s| s.contains("ok")).unwrap_or(false)
    }
}

pub fn find_adb() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("ADB") {
        let p = PathBuf::from(configured);
        if p.is_file() {
            return Some(p);
        }
    }
    let managed = crate::paths::adb_dir().join("adb.exe");
    if managed.is_file() {
        return Some(managed);
    }
    crate::util::which("adb")
}

/// A running QEMU process.
pub struct QemuSession {
    pub child: Child,
    pub options: SessionOptions,
    pub log_path: PathBuf,
    pub adb_port: u16,
}

impl QemuSession {
    /// Launch QEMU (the window opens once the guest boots).
    pub fn start(install: &QemuInstall, options: SessionOptions) -> Result<QemuSession, QemuError> {
        crate::paths::ensure(&crate::paths::logs_dir()).ok();
        let log_path = crate::paths::logs_dir().join(format!(
            "{}.log",
            crate::util::sanitize_file_name(&options.session_name)
        ));
        let log = std::fs::File::create(&log_path).map_err(|e| QemuError::Launch(e.to_string()))?;
        let log_err = log.try_clone().map_err(|e| QemuError::Launch(e.to_string()))?;

        let accel = detect_accel();
        let args = options.build_args(accel);
        let mut cmd = Command::new(&install.exe);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        // QEMU needs the folder of its DLLs on PATH when the portable build is used.
        if let Some(dir) = install.exe.parent() {
            let mut path = std::env::var_os("PATH").unwrap_or_default();
            let mut parts = vec![dir.to_path_buf()];
            parts.extend(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(parts) {
                path = joined;
            }
            cmd.env("PATH", path);
        }
        let mut child = cmd.spawn().map_err(|e| QemuError::Launch(e.to_string()))?;

        // If QEMU dies within a second it is almost always a host problem
        // (audio device, GPU, acceleration). Retry once in a conservative
        // "safe mode" so the app still opens.
        std::thread::sleep(std::time::Duration::from_millis(900));
        if let Ok(Some(status)) = child.try_wait() {
            let log_text = std::fs::read_to_string(&log_path).unwrap_or_default();
            let mut safe = options.clone();
            safe.audio = false;
            safe.gpu = GpuMode::Swiftshader;
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_path)
                .map(|mut f| {
                    use std::io::Write;
                    let _ = writeln!(
                        f,
                        "\n--- QEMU exited immediately ({status}); retrying in safe mode ---\n{log_text}"
                    );
                });
            let retry_args = safe.build_args(Accel::Tcg);
            let log2 = std::fs::File::create(&log_path).map_err(|e| QemuError::Launch(e.to_string()))?;
            let log2_err = log2.try_clone().map_err(|e| QemuError::Launch(e.to_string()))?;
            let mut retry = Command::new(&install.exe);
            retry
                .args(&retry_args)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log2))
                .stderr(Stdio::from(log2_err));
            child = retry.spawn().map_err(|e| QemuError::Launch(e.to_string()))?;
        }
        let adb_port = options.adb_port;
        Ok(QemuSession {
            child,
            options,
            log_path,
            adb_port,
        })
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait()
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Send a command to the QEMU monitor (used for clean shutdown and snapshots).
    pub fn monitor_command(&self, command: &str) -> Result<String, QemuError> {
        let addr = format!("127.0.0.1:{}", self.adb_port + 1);
        let mut stream = TcpStream::connect(&addr).map_err(|e| QemuError::Adb(e.to_string()))?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        // Read the monitor banner first.
        let mut reader = BufReader::new(stream.try_clone().map_err(|e| QemuError::Adb(e.to_string()))?);
        let mut banner = String::new();
        let _ = reader.read_line(&mut banner);
        stream
            .write_all(format!("{command}\n").as_bytes())
            .map_err(|e| QemuError::Adb(e.to_string()))?;
        Ok(banner)
    }

    /// QEMU's own pid, used by the app coordinator.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Ask the guest to shut down (ACPI), then kill if it does not react.
    pub fn shutdown(&mut self, grace: Duration) {
        let _ = self.monitor_command("system_powerdown");
        let start = Instant::now();
        while start.elapsed() < grace {
            if !self.is_running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        self.kill();
    }

    pub fn log_tail(&self, lines: usize) -> String {
        let Ok(text) = std::fs::read_to_string(&self.log_path) else {
            return String::new();
        };
        let all: Vec<&str> = text.lines().collect();
        let start = all.len().saturating_sub(lines);
        all[start..].join("\n")
    }
}

/// Free TCP port helper for sessions (adb + monitor + qmp need 3 ports).
pub fn allocate_ports() -> u16 {
    use std::net::TcpListener;
    for base in (5600u16..6400).step_by(4) {
        let mut ok = true;
        for offset in 0..3 {
            if TcpListener::bind(("127.0.0.1", base + offset)).is_err() {
                ok = false;
                break;
            }
        }
        if ok {
            return base;
        }
    }
    5600
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SessionOptions {
        SessionOptions {
            image: PathBuf::from(r"C:\images\android-x86.iso"),
            memory_mb: 2048,
            cores: 2,
            resolution: (1080, 1920),
            dpi: 320,
            gpu: GpuMode::Swiftshader,
            display_mode: DisplayMode::Window,
            window_size: (540, 960),
            adb_port: 5600,
            extra_args: vec![],
            snapshot: false,
            audio: true,
            session_name: "demo app".into(),
        }
    }

    #[test]
    fn builds_sane_command_line() {
        let args = opts().build_args(Accel::Tcg);
        let joined = args.join(" ");
        assert!(joined.contains("-m 2048"));
        assert!(joined.contains("-smp 2"));
        assert!(joined.contains("hostfwd=tcp:127.0.0.1:5600-:5555"));
        assert!(joined.contains("xres=540,yres=960"), "window size drives the screen: {joined}");
        assert!(joined.contains("tcg,thread=multi"));
        assert!(joined.contains("system-x86_64") == false);
        // display args must be balanced
        assert_eq!(
            args.iter().filter(|a| *a == "-display").count(),
            1,
            "exactly one display option"
        );
    }

    #[test]
    fn headless_has_no_window() {
        let mut o = opts();
        o.display_mode = DisplayMode::Headless;
        let args = o.build_args(Accel::Whpx).join(" ");
        assert!(args.contains("-display none"));
        assert!(args.contains("whpx"));
    }

    #[test]
    fn safe_overlay_names() {
        assert_eq!(safe_overlay_name("a/b:c"), "a_b_c");
    }
}
