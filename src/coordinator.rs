//! Session coordination: all apps share one Android VM.
//!
//! Opening a second app shortcut attaches to the VM that is already running
//! (instant, no second boot) and simply starts the activity inside it. The lock
//! file keeps track of the running QEMU process.

use crate::json::{self, Value};
use crate::qemu::{self, AdbClient};
use crate::store::{AppEntry, DisplayMode, GpuMode, KeepDecision};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub pid: u32,
    pub adb_port: u16,
    pub started_at: u64,
    pub image: String,
    pub slug: String,
}

impl SessionInfo {
    pub fn to_json(&self) -> Value {
        let mut v = Value::new_object();
        v.set("pid", Value::Int(self.pid as i64));
        v.set("adbPort", Value::Int(self.adb_port as i64));
        v.set("startedAt", Value::Int(self.started_at as i64));
        v.set("image", Value::Str(self.image.clone()));
        v.set("slug", Value::Str(self.slug.clone()));
        v
    }

    pub fn from_json(v: &Value) -> Option<SessionInfo> {
        Some(SessionInfo {
            pid: v.get("pid")?.as_u32()?,
            adb_port: v.get("adbPort")?.as_u32()? as u16,
            started_at: v.get("startedAt").and_then(|x| x.as_u64()).unwrap_or(0),
            image: v
                .get("image")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            slug: v.get("slug").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        })
    }
}

pub fn lock_path() -> PathBuf {
    crate::paths::data_dir().join("session.lock")
}

pub fn read_live_session() -> Option<SessionInfo> {
    let text = std::fs::read_to_string(lock_path()).ok()?;
    let value = json::parse(&text).ok()?;
    let info = SessionInfo::from_json(&value)?;
    if crate::win::process_is_alive(info.pid) {
        Some(info)
    } else {
        // Stale lock from a crashed/killed VM.
        let _ = std::fs::remove_file(lock_path());
        None
    }
}

pub fn write_session(info: &SessionInfo) {
    if let Some(parent) = lock_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(lock_path(), info.to_json().to_pretty());
}

pub fn clear_session() {
    let _ = std::fs::remove_file(lock_path());
}

/// Outcome of "make sure a VM is up".
pub enum SessionState {
    /// We started the VM ourselves; the caller should stay alive while it runs.
    Started(qemu::QemuSession),
    /// An existing VM was reused.
    Attached(SessionInfo),
}

/// Start or attach to the shared VM, waiting for Android to finish booting.
pub fn ensure_session(entry: &AppEntry) -> Result<SessionState, String> {
    if let Some(info) = read_live_session() {
        if tcp_open(info.adb_port) {
            println!("using the already running emulator (adb port {})", info.adb_port);
            return Ok(SessionState::Attached(info));
        }
        // Process alive but adb not reachable yet: wait a little for it.
        if wait_port(info.adb_port, Duration::from_secs(90)) {
            return Ok(SessionState::Attached(info));
        }
    }

    let install = qemu::detect().ok_or_else(|| {
        "QEMU was not found. Run `android-emu setup` for instructions, then retry.".to_string()
    })?;
    let registry = crate::config::ImageRegistry::load();
    let image = registry
        .active_image()
        .cloned()
        .ok_or_else(|| {
            "no Android system image installed yet\n\
             add one with: android-emu image add <android-x86.iso|.img|.qcow2>"
                .to_string()
        })?;
    let settings = crate::config::Settings::load();

    let port = allocate_base_port(settings.adb_port);
    let mut options = qemu::SessionOptions::from_entry(entry, image.path.clone(), port);
    options.memory_mb = settings.memory_mb;
    options.cores = settings.cores;
    if entry.display_mode == DisplayMode::Window && entry.gpu == GpuMode::Off {
        options.display_mode = DisplayMode::Headless;
    }
    options.snapshot = false;

    println!(
        "starting Android ({} image {}, {} MB RAM, {} cores, {})",
        image.kind.as_str(),
        image.name,
        options.memory_mb,
        options.cores,
        qemu::detect_accel().label()
    );
    let mut session =
        qemu::QemuSession::start(&install, options).map_err(|e| e.to_string())?;
    println!("QEMU log: {}", session.log_path.display());

    let info = SessionInfo {
        pid: session.pid(),
        adb_port: session.adb_port,
        started_at: now(),
        image: image.name.clone(),
        slug: entry.slug.clone(),
    };
    write_session(&info);

    // Wait for the guest's adb port, then for Android to report boot completed.
    if !wait_port(session.adb_port, boot_timeout()) {
        let tail = session.log_tail(15);
        // A guest that never opens adb is unusable: stop it and drop the lock so
        // the next attempt starts a clean VM instead of attaching to a dead one.
        session.kill();
        clear_session();
        return Err(format!(
            "Android did not come up in time.\nCheck the log: {}\nLast QEMU output:\n{tail}",
            session.log_path.display()
        ));
    }
    match AdbClient::detect(session.adb_port) {
        Ok(adb) => {
            let _ = adb.connect();
            println!("waiting for Android to finish booting...");
            let booted = adb.wait_for_boot(boot_timeout(), |_m| {});
            if !booted {
                println!("warning: boot flag not set yet, continuing anyway");
            }
        }
        Err(_) => {
            println!("warning: adb not found - install platform-tools to push APKs automatically");
        }
    }
    Ok(SessionState::Started(session))
}

/// How long to wait for the guest's adb port (override with
/// ANDROID_EMU_BOOT_TIMEOUT seconds; useful for slow software emulation).
fn boot_timeout() -> Duration {
    let secs = std::env::var("ANDROID_EMU_BOOT_TIMEOUT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(180);
    Duration::from_secs(secs.clamp(10, 3600))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn tcp_open(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(400),
    )
    .is_ok()
}

fn wait_port(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if tcp_open(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

fn allocate_base_port(preferred: u16) -> u16 {
    use std::net::TcpListener;
    let mut port = preferred.max(1024) & !3;
    for _ in 0..128 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok()
            && TcpListener::bind(("127.0.0.1", port + 1)).is_ok()
        {
            return port;
        }
        port += 4;
    }
    qemu::allocate_ports()
}

/// What to do with the VM after the app has been started.
pub fn decide_keep(entry: &AppEntry, state: &SessionState) -> KeepDecision {
    match state {
        SessionState::Attached(_) => KeepDecision::Detach,
        SessionState::Started(_) => {
            if entry.keep_running {
                KeepDecision::Detach
            } else {
                KeepDecision::Wait
            }
        }
    }
}
