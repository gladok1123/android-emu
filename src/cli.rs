//! `android-emu` command line interface.
//!
//! Typical flows:
//! * double-click an `.apk`  -> `android-emu --install <file.apk>`
//! * double-click a shortcut -> `android-emu --run "<app slug>"`

use crate::config::{ImageKind, ImageRegistry, Settings};
use crate::installer::{self, InstallRequest};
use crate::qemu::{self, AdbClient};
use crate::remote;
use crate::store::{self, DisplayMode, GpuMode, InstallOptions};
use crate::util::human_bytes;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(args: Vec<String>) -> i32 {
    let mut args = args;
    if args.is_empty() {
        print_help();
        return 0;
    }
    // Windows passes the program name as the first argument.
    if args[0].to_lowercase().ends_with(".exe") || args[0].contains("android-emu") {
        args.remove(0);
    }
    if args.is_empty() {
        print_help();
        return 0;
    }

    let command = args[0].clone();
    let rest = args[1..].to_vec();
    let result = match command.as_str() {
        "--install" | "-i" | "install" => cmd_install(rest),
        "--run" | "run" => cmd_run(rest),
        "list" | "ls" => cmd_list(),
        "info" => cmd_info(rest),
        "remove" | "uninstall" => cmd_remove(rest),
        "config" => cmd_config(rest),
        "image" | "images" => cmd_image(rest),
        "setup" => cmd_setup(rest),
        "doctor" | "check" => cmd_doctor(),
        "arm" => cmd_arm(rest),
        "assoc" => cmd_assoc(rest),
        "open" | "launch" => cmd_open(rest),
        "adb" => cmd_adb(rest),
        "phone" | "start" => cmd_phone(rest),
        "help" | "--help" | "-h" | "/?" => {
            print_help();
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("android-emu {VERSION}");
            Ok(())
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("run `android-emu help` for the list of commands");
            Err(ExitCode(2))
        }
    };
    match result {
        Ok(()) => 0,
        Err(ExitCode(code)) => code,
    }
}

#[derive(Debug)]
struct ExitCode(i32);

type CmdResult = Result<(), ExitCode>;

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode(1)
}

fn ok_exit() -> ExitCode {
    ExitCode(0)
}

pub fn print_help_public() {
    print_help();
}

fn print_help() {
    println!(
        r#"android-emu {version} - lightweight Android emulator for Windows (QEMU + Android-x86)

USAGE
  android-emu <command> [arguments]

INSTALLING APPS
  --install <file.apk>        Install an APK: copies it, extracts the icon and puts a
                              shortcut on the Desktop + Start Menu (this is what happens
                              when you double-click an .apk after `android-emu assoc`)
  --run <app> [--detach]      Start the emulator window for an installed app
  phone [--size 480x880]      Boot Android on its own (use it like a phone/tablet)
  open <app>                  Same as --run, but keeps running in background
  list                        Show installed apps
  info <app>                  Show details about an app
  remove <app>                Uninstall an app and delete its shortcuts

WINDOW / DISPLAY
  config <app> --size 540x960         Window size used when the app opens
  config <app> --resolution 720x1280  Emulated screen resolution
  config <app> --dpi 240              Emulated screen density
  config <app> --gpu swiftshader|angle|guest|off
  config <app> --mode window|fullscreen|headless
  config <app> --extra-arg=-smp --extra-arg=4
  config <app> --keep-running true    Keep the VM alive after closing the window

ANDROID IMAGES
  image list                  Installed Android system images
  image add <file.iso|img|qcow2> [--name NAME] [--use]
                              Register your own Android-x86 / BlissOS / PrimeOS image
  image use <name>            Pick the image used for new sessions
  image remove <name>         Forget an image (deletes the managed copy)
  image catalog [filter]      List downloadable Android images (ISO / disk)
  image get "<name>"          Download an image from the catalog and activate it
  image overlay [name]        Create a writable per-app overlay for persistence

SETUP
  gui                         Open the settings window (also: no arguments)
  setup                       Enable .apk file association (double-click install)
  setup --download-qemu       Explain how to obtain QEMU (link + expected layout)
  assoc on|off|status         Manage the .apk association
  doctor                      Diagnose QEMU, acceleration, adb and images
  adb <args>                  Run adb against the running emulator

ARM APPLICATIONS
  arm status                  Is ARM translation (libhoudini/libndk) active in the guest?
  arm enable                  Try to switch ARM translation on inside the guest
  arm install <module.zip>    Install a libndk/houdini module through Magisk
  arm check <file.apk>        Does this APK contain only ARM code?

Everything is stored in %LOCALAPPDATA%\android-emu.
"#,
        version = VERSION
    );
}

fn cmd_install(args: Vec<String>) -> CmdResult {
    // Called by Explorer: `android-emu --install "C:\path\app.apk"`.
    let mut apk: Option<PathBuf> = None;
    let mut window: Option<(u32, u32)> = None;
    let mut resolution: Option<String> = None;
    let mut gpu: Option<GpuMode> = None;
    let mut mode: Option<DisplayMode> = None;
    let mut no_shortcut = false;
    let mut force_shortcut = false;
    let mut launch = true;

    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--size" if i + 1 < args.len() => {
                window = store::parse_resolution(&args[i + 1]);
                if window.is_none() {
                    eprintln!("warning: ignoring invalid --size {}", args[i + 1]);
                } else {
                    // The emulated screen drives the window size: keep both in
                    // sync unless `--resolution` is given explicitly.
                    resolution = Some(args[i + 1].clone());
                }
                i += 1;
            }
            "--resolution" if i + 1 < args.len() => {
                resolution = Some(args[i + 1].clone());
                i += 1;
            }
            "--gpu" if i + 1 < args.len() => {
                gpu = Some(GpuMode::from_str(&args[i + 1]));
                i += 1;
            }
            "--mode" if i + 1 < args.len() => {
                mode = Some(DisplayMode::from_str(&args[i + 1]));
                i += 1;
            }
            "--no-shortcut" => no_shortcut = true,
            "--shortcut" => force_shortcut = true,
            "--no-launch" => launch = false,
            other if !other.starts_with('-') && apk.is_none() => apk = Some(PathBuf::from(other)),
            other => eprintln!("warning: unknown option {other}"),
        }
        i += 1;
    }

    let Some(apk) = apk else {
        eprintln!("usage: android-emu --install <file.apk>");
        return Err(ExitCode(2));
    };

    let settings = Settings::load();
    let request = InstallRequest {
        apk: apk.clone(),
        options: InstallOptions {
            window,
            resolution,
            dpi: None,
            gpu,
            display_mode: mode,
        },
        desktop_shortcut: (settings.create_desktop_shortcut || force_shortcut) && !no_shortcut,
        start_menu_shortcut: (settings.create_start_menu_shortcut || force_shortcut) && !no_shortcut,
    };

    match installer::install_to_disk(&request) {
        Ok(installed) => {
            println!("{}", installer::summarize(&installed));
            if launch && settings.autostart_vm {
                let entry = installed.entry.clone();
                println!(
                    "\nStarting Android... (first boot can take a few minutes)"
                );
                match installer::launch_app(&entry, true) {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("could not start the emulator: {e}");
                        eprintln!("the app is installed - start it again from the desktop shortcut");
                        // An install must not be reported as failed because the VM
                        // could not boot (missing image is a common first-run case).
                        return Ok(());
                    }
                }
            } else {
                println!("\nStart it with: android-emu --run \"{}\"", installed.entry.slug);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("\nInstallation failed: {e}");
            eprintln!("Nothing was changed. Fix the problem above and double-click the APK again.");
            #[cfg(windows)]
            crate::win::shortcut::shell_open(&crate::paths::data_dir())
                .ok();
            Err(fail(e))
        }
    }
}

fn resolve_app(query: &str) -> Result<store::AppEntry, ExitCode> {
    store::find_fuzzy(query).ok_or_else(|| {
        eprintln!("no installed app matches '{query}'");
        eprintln!("run `android-emu list` to see installed apps");
        ExitCode(1)
    })
}

fn cmd_run(args: Vec<String>) -> CmdResult {
    let detach = args.iter().any(|a| a == "--detach" || a == "-d");
    let Some(query) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("usage: android-emu --run <app> [--detach]");
        return Err(ExitCode(2));
    };
    let mut entry = resolve_app(query)?;
    if detach {
        entry.keep_running = true;
    }
    match installer::launch_app(&entry, true) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("\nCould not start \"{}\": {e}", entry.label);
            eprintln!(
                "Tip: run `android-emu doctor` - it checks QEMU, the Android image and adb."
            );
            Err(fail(e))
        }
    }
}

fn cmd_open(args: Vec<String>) -> CmdResult {
    cmd_run(args)
}

fn cmd_list() -> CmdResult {
    let apps = store::list();
    if apps.is_empty() {
        println!("No apps installed yet.");
        println!("Double-click an .apk (after `android-emu setup`) or run:");
        println!("  android-emu --install MyApp.apk");
        return Ok(());
    }
    println!("{:<32} {:<28} {:<11} {}", "APP", "PACKAGE", "VERSION", "WINDOW");
    println!("{}", "-".repeat(92));
    for app in apps {
        println!(
            "{:<32} {:<28} {:<10} {}x{}",
            truncate(&app.label, 31),
            truncate(&app.package, 27),
            truncate(&app.version_name, 10),
            app.window_width,
            app.window_height
        );
    }
    println!("\n{} app(s) in {}", store::list().len(), crate::paths::apps_dir().display());
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.pop();
        out.push('…');
    }
    out
}

fn cmd_info(args: Vec<String>) -> CmdResult {
    let Some(query) = args.first() else {
        eprintln!("usage: android-emu info <app>");
        return Err(ExitCode(2));
    };
    let app = resolve_app(query)?;
    println!("{}", app.label);
    println!("{}", "=".repeat(app.label.chars().count().max(3)));
    println!("package:    {}", app.package);
    println!("version:    {} ({})", app.version_name, app.version_code);
    println!("size:       {}", human_bytes(app.size_bytes));
    println!("sdk:        min {:?} target {:?}", app.min_sdk, app.target_sdk);
    println!("launch:     {}", app.launch_component.as_deref().unwrap_or("(none)"));
    println!("directory:  {}", app.dir().display());
    println!(
        "window:     {}x{}  screen {} @ {} dpi  gpu={} mode={}",
        app.window_width,
        app.window_height,
        app.resolution,
        app.dpi,
        app.gpu.as_str(),
        app.display_mode.as_str()
    );
    Ok(())
}

fn cmd_remove(args: Vec<String>) -> CmdResult {
    let Some(query) = args.first() else {
        eprintln!("usage: android-emu remove <app>");
        return Err(ExitCode(2));
    };
    let app = resolve_app(query)?;
    // Remove shortcuts first so they do not point at a missing app.
    if let Some(desktop) = crate::paths::desktop_dir() {
        let _ = std::fs::remove_file(desktop.join(format!(
            "{}.lnk",
            crate::util::sanitize_file_name(&app.label)
        )));
    }
    if let Some(menu) = crate::paths::start_menu_dir() {
        let _ = std::fs::remove_file(menu.join(format!(
            "{}.lnk",
            crate::util::sanitize_file_name(&app.label)
        )));
    }
    match store::uninstall(&app.slug) {
        Ok(true) => {
            println!("removed {} ({})", app.label, app.package);
            Ok(())
        }
        Ok(false) => Err(fail("nothing was removed")),
        Err(e) => Err(fail(e)),
    }
}

fn cmd_config(args: Vec<String>) -> CmdResult {
    let Some(query) = args.first() else {
        eprintln!("usage: android-emu config <app> [--size WxH] [--resolution WxH] [--dpi N] [--gpu MODE] [--mode MODE]");
        return Err(ExitCode(2));
    };
    let mut app = resolve_app(query)?;
    let mut i = 1;
    let mut changed = false;
    while i < args.len() {
        let key = &args[i];
        let value = args.get(i + 1).cloned();
        match (key.as_str(), value) {
            ("--size", Some(v)) => {
                match store::parse_resolution(&v) {
                    Some((w, h)) => {
                        app.window_width = w;
                        app.window_height = h;
                        changed = true;
                    }
                    None => return Err(fail(format!("invalid size '{v}' (use e.g. 540x960)"))),
                }
                i += 1;
            }
            ("--resolution", Some(v)) => {
                if store::parse_resolution(&v).is_none() {
                    return Err(fail(format!("invalid resolution '{v}'")));
                }
                app.resolution = v;
                changed = true;
                i += 1;
            }
            ("--dpi", Some(v)) => {
                let dpi: u32 = v.parse().map_err(|_| fail("invalid dpi"))?;
                app.dpi = dpi.clamp(80, 640);
                changed = true;
                i += 1;
            }
            ("--gpu", Some(v)) => {
                app.gpu = GpuMode::from_str(&v);
                changed = true;
                i += 1;
            }
            ("--mode", Some(v)) => {
                app.display_mode = DisplayMode::from_str(&v);
                changed = true;
                i += 1;
            }
            ("--keep-running", Some(v)) => {
                app.keep_running = v == "true" || v == "1";
                changed = true;
                i += 1;
            }
            (k, _) if k.starts_with("--extra-arg=") => {
                let arg = k.trim_start_matches("--extra-arg=").to_string();
                app.extra_args.push(arg);
                changed = true;
            }
            (k, _) => return Err(fail(format!("unknown option {k}"))),
        }
        i += 1;
    }
    if !changed {
        println!("{}", serde_like(&app));
        return Ok(());
    }
    store::save_entry_config(&app).map_err(|e| fail(e))?;
    let mut index = store::load_index();
    if let Some(pos) = index.apps.iter().position(|a| a.slug == app.slug) {
        index.apps[pos] = app.clone();
    }
    store::save_index(&index).map_err(|e| fail(e))?;
    println!(
        "updated {}: window {}x{}, screen {}, gpu={}, mode={}",
        app.label,
        app.window_width,
        app.window_height,
        app.resolution,
        app.gpu.as_str(),
        app.display_mode.as_str()
    );
    Ok(())
}

fn serde_like(app: &store::AppEntry) -> String {
    app.to_json().to_pretty()
}

fn cmd_image(args: Vec<String>) -> CmdResult {
    let Some(sub) = args.first() else {
        return cmd_image_list();
    };
    match sub.as_str() {
        "list" | "ls" => cmd_image_list(),
        "add" => {
            let Some(path) = args.get(1) else {
                return Err(fail("usage: android-emu image add <file.iso|img|qcow2> [--name NAME] [--use]"));
            };
            let mut name = None;
            let mut use_it = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--name" if i + 1 < args.len() => {
                        name = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--use" => use_it = true,
                    other => return Err(fail(format!("unknown option {other}"))),
                }
                i += 1;
            }
            let path = PathBuf::from(path);
            let mut registry = ImageRegistry::load();
            match registry.add(&path, name.clone()) {
                Ok(entry) => {
                    println!(
                        "added image '{}' ({}, {})",
                        entry.name,
                        entry.kind.as_str(),
                        human_bytes(entry.size_bytes)
                    );
                    println!("  stored at {}", entry.path.display());
                    if use_it {
                        registry.set_active(&entry.name).map_err(|e| fail(e))?;
                        println!("  now the active image");
                    }
                    if entry.kind == ImageKind::Iso {
                        println!("\nNote: an ISO boots read-only (changes are lost on reboot).");
                        println!("For a persistent Android install, use a .img/.qcow2 disk image.");
                    }
                    Ok(())
                }
                Err(e) => Err(fail(e)),
            }
        }
        "use" => {
            let Some(name) = args.get(1) else {
                return Err(fail("usage: android-emu image use <name>"));
            };
            let mut registry = ImageRegistry::load();
            registry.set_active(name).map_err(|e| fail(e))?;
            println!("active image: {name}");
            Ok(())
        }
        "remove" => {
            let Some(name) = args.get(1) else {
                return Err(fail("usage: android-emu image remove <name>"));
            };
            let mut registry = ImageRegistry::load();
            registry.remove(name).map_err(|e| fail(e))?;
            println!("removed image {name}");
            Ok(())
        }
        "catalog" | "get" | "download" => cmd_image_catalog(args.get(1).cloned(), args.get(2).cloned()),
        "overlay" => {
            println!("Per-app overlays are created automatically on first start.");
            println!("They live in {}", crate::paths::images_dir().join("overlays").display());
            Ok(())
        }
        other => Err(fail(format!("unknown image command {other}"))),
    }
}

fn cmd_image_catalog(filter: Option<String>, name: Option<String>) -> CmdResult {
    let url = std::env::var("ANDROID_EMU_CATALOG").unwrap_or_else(|_| remote::DEFAULT_CATALOG_URL.to_string());
    println!("fetching catalog: {url}");
    let images = match remote::fetch_catalog(&url) {
        Ok(list) => list,
        Err(e) => {
            return Err(fail(format!(
                "{e}\n\nNo internet? Download an image manually and run:\n  android-emu image add <file.iso>"
            )))
        }
    };
    if let Some(name) = name {
        let Some(image) = images
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(&name))
        else {
            return Err(fail(format!("no image named '{name}' in the catalog")));
        };
        println!(
            "downloading {} ({}) ...",
            image.name,
            crate::util::human_bytes(image.size_bytes)
        );
        let mut last_report = 0u64;
        match remote::download_image(image, |downloaded| {
            if downloaded - last_report > 64 * 1024 * 1024 {
                last_report = downloaded;
                eprintln!("  {} downloaded", crate::util::human_bytes(downloaded));
            }
        }) {
            Ok(path) => {
                println!("downloaded to {}", path.display());
                if !remote::looks_like_disk_image(&path) {
                    println!("warning: the file does not look like a bootable image");
                }
                let mut registry = ImageRegistry::load();
                match registry.add(&path, None) {
                    Ok(entry) => {
                        registry.set_active(&entry.name).map_err(|e| fail(e))?;
                        println!("added as '{}' and set active", entry.name);
                        Ok(())
                    }
                    Err(e) => Err(fail(e)),
                }
            }
            Err(e) => Err(fail(e)),
        }
    } else {
        let filtered: Vec<&remote::RemoteImage> = match &filter {
            Some(f) => images
                .iter()
                .filter(|i| {
                    i.name.to_lowercase().contains(&f.to_lowercase())
                        || i.description.to_lowercase().contains(&f.to_lowercase())
                })
                .collect(),
            None => images.iter().collect(),
        };
        if filtered.is_empty() {
            println!("no matching images");
            return Ok(());
        }
        println!("{:<28} {:<7} {:<10} {}", "NAME", "ARCH", "SIZE", "DESCRIPTION");
        println!("{}", "-".repeat(96));
        for img in filtered {
            println!(
                "{:<28} {:<7} {:<10} {}",
                truncate(&img.name, 27),
                img.arch,
                crate::util::human_bytes(img.size_bytes),
                truncate(&img.description, 50)
            );
        }
        println!("\nInstall one with:  android-emu image get \"<NAME>\"");
        Ok(())
    }
}

fn cmd_image_list() -> CmdResult {
    let registry = ImageRegistry::load();
    if registry.images.is_empty() {
        println!("No Android images installed.");
        println!("\nAdd your own Android-x86 / BlissOS / PrimeOS image:");
        println!("  android-emu image add D:\\Downloads\\android-x86_64-9.0-r2.iso");
        println!("\nA writable disk image (installed Android) gives persistence:");
        println!("  android-emu image add D:\\VM\\android.img");
        return Ok(());
    }
    println!("{:<24} {:<8} {:<10} {}", "NAME", "TYPE", "SIZE", "PATH");
    println!("{}", "-".repeat(80));
    for img in &registry.images {
        let active = registry.active.as_deref() == Some(img.name.as_str());
        println!(
            "{:<24} {:<8} {:<10} {}{}",
            truncate(&img.name, 23),
            img.kind.as_str(),
            human_bytes(img.size_bytes),
            img.path.display(),
            if active { "  [active]" } else { "" }
        );
    }
    Ok(())
}

fn cmd_setup(args: Vec<String>) -> CmdResult {
    let mut download_qemu = false;
    for a in &args {
        if a == "--download-qemu" {
            download_qemu = true;
        }
    }
    #[cfg(windows)]
    let exe = crate::win::exe_path();
    #[cfg(windows)]
    {
        match crate::win::fileassoc::register_apk_association(&exe) {
            Ok(()) => println!("registered .apk -> android-emu (double-click an APK to install)"),
            Err(e) => {
                eprintln!("could not register the .apk association: {e}");
                eprintln!("(this only needs the current user's registry, HKCU)");
            }
        }
    }
    #[cfg(not(windows))]
    println!("file association is only available on Windows");

    match qemu::detect() {
        Some(q) => println!("QEMU found: {} ({})", q.exe.display(), q.version),
        None => {
            println!("\nQEMU is not installed yet.");
            println!("Download the official Windows build (qemu-system-x86_64.exe) from");
            println!("  https://qemu.weilnetz.de/w64/");
            println!("and either put it on PATH or copy the folder to");
            println!("  {}", crate::paths::qemu_dir().display());
        }
    }
    if let Some(adb) = qemu::find_adb() {
        println!("adb found: {}", adb.display());
    } else {
        println!("\nadb not found - needed to install APKs into the VM automatically.");
        println!("Get platform-tools from https://developer.android.com/tools/releases/platform-tools");
        println!("and put adb.exe into {}", crate::paths::adb_dir().display());
    }
    if download_qemu {
        println!("\nAutomatic download helper: place the QEMU installer archive in");
        println!("  {}", crate::paths::qemu_dir().display());
        println!("the launcher picks up qemu-system-x86_64.exe automatically.");
    }
    let _ = ok_exit();
    Ok(())
}

fn cmd_assoc(args: Vec<String>) -> CmdResult {
    let action = args.first().map(|s| s.as_str()).unwrap_or("status");
    match action {
        "on" => cmd_setup(vec![]),
        "off" => {
            #[cfg(windows)]
            crate::win::fileassoc::unregister_apk_association();
            println!("removed the .apk association");
            Ok(())
        }
        _ => {
            #[cfg(windows)]
            {
                if crate::win::fileassoc::is_registered() {
                    println!(".apk files are associated with android-emu");
                    if let Some(cmd) = crate::win::fileassoc::registered_command() {
                        println!("  command: {cmd}");
                    }
                } else {
                    println!(".apk files are NOT associated yet - run `android-emu assoc on`");
                }
            }
            Ok(())
        }
    }
}

/// Boot Android without a specific app ("use it like a phone").
fn cmd_phone(args: Vec<String>) -> CmdResult {
    let mut width = 0u32;
    let mut height = 0u32;
    let mut fullscreen = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--size" if i + 1 < args.len() => {
                if let Some((w, h)) = store::parse_resolution(&args[i + 1]) {
                    width = w;
                    height = h;
                }
                i += 1;
            }
            "--fullscreen" => fullscreen = true,
            other => eprintln!("warning: unknown option {other}"),
        }
        i += 1;
    }
    let entry = store::AppEntry {
        slug: "android".into(),
        package: "android".into(),
        label: "Android".into(),
        version_name: String::new(),
        version_code: 0,
        launch_component: None,
        min_sdk: None,
        target_sdk: None,
        size_bytes: 0,
        installed_at: 0,
        window_width: if width > 0 { width } else { 480 },
        window_height: if height > 0 { height } else { 880 },
        resolution: "1080x1920".into(),
        dpi: 320,
        display_mode: if fullscreen {
            DisplayMode::Fullscreen
        } else {
            DisplayMode::Window
        },
        gpu: GpuMode::from_str(&Settings::load().gpu),
        extra_args: Vec::new(),
        keep_running: false,
    };
    match installer::launch_app(&entry, false) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("\nCould not start Android: {e}");
            eprintln!("Run `android-emu doctor` to see what is missing.");
            Err(fail(e))
        }
    }
}

/// `android-emu arm status|enable|install <module.zip>`
///
/// ARM-only APKs cannot execute on x86 Android without a translator. These
/// commands inspect and configure libhoudini (Android-x86/BlissOS) or libndk
/// (BlissOS 12+) inside the running guest.
fn cmd_arm(args: Vec<String>) -> CmdResult {
    let action = args.first().map(|s| s.as_str()).unwrap_or("status");
    let settings = Settings::load();
    let session = crate::coordinator::read_live_session();
    let port = session.as_ref().map(|s| s.adb_port).unwrap_or(settings.adb_port);

    match action {
        "status" => {
            println!("ARM translation is {}", if settings.arm_translation { "enabled" } else { "disabled" });
            if session.is_none() {
                println!("Android is not running - start an app first to inspect the guest.");
                println!("{}", crate::arm::guest_support_hint().unwrap_or_default());
                return Ok(());
            }
            let adb = AdbClient::detect(port).map_err(|e| fail(e))?;
            adb.connect().map_err(|e| fail(e))?;
            let status = crate::arm::probe(&adb);
            println!("adb port:  {port}");
            println!("abi list:  {}", status.abilist);
            println!("translator: {}", status.translator.unwrap_or_else(|| "none".into()));
            println!("ready:     {}", status.translation_ready);
            if !status.translation_ready {
                println!("\nRun `android-emu arm enable` to try activating it, or use a");
                println!("BlissOS/PrimeOS image (houdini/libndk included).");
            }
            Ok(())
        }
        "enable" | "on" => {
            let adb = AdbClient::detect(port).map_err(|e| fail(e))?;
            adb.connect().map_err(|e| fail(e))?;
            match crate::arm::enable_in_guest(&adb) {
                Ok(message) => {
                    println!("{message}");
                    Ok(())
                }
                Err(message) => Err(fail(message)),
            }
        }
        "install" => {
            let Some(path) = args.get(1) else {
                return Err(fail(
                    "usage: android-emu arm install <libndk-or-houdini-module.zip>\n\
                     (module zips are installed through Magisk inside the guest)",
                ));
            };
            let adb = AdbClient::detect(port).map_err(|e| fail(e))?;
            adb.connect().map_err(|e| fail(e))?;
            match crate::arm::install_payload(&adb, std::path::Path::new(path)) {
                Ok(message) => {
                    println!("{message}");
                    Ok(())
                }
                Err(message) => Err(fail(message)),
            }
        }
        "check" => {
            let Some(path) = args.get(1) else {
                return Err(fail("usage: android-emu arm check <file.apk>"));
            };
            let info = crate::apk::parse_apk(std::path::Path::new(path)).map_err(|e| fail(e))?;
            println!("{} ({})", info.label, info.package);
            println!("ABIs: {}", if info.abis.is_empty() { "none (pure Java)".to_string() } else { info.abis.join(", ") });
            println!("{}", crate::arm::describe_apk(&info));
            Ok(())
        }
        other => Err(fail(format!("unknown arm command '{other}' (status|enable|install|check)"))),
    }
}

fn cmd_doctor() -> CmdResult {
    println!("android-emu {VERSION} - diagnostics\n");
    println!("data directory: {}", crate::paths::data_dir().display());
    match qemu::detect() {
        Some(q) => println!("QEMU:           {} ({:?})\n                {}", q.version, q.source, q.exe.display()),
        None => println!("QEMU:           NOT FOUND (run `android-emu setup`)"),
    }
    println!("acceleration:   {}", qemu::detect_accel().label());
    #[cfg(windows)]
    for (k, v) in crate::win::system::summary() {
        println!("{:<15} {}", format!("{k}:"), v);
    }
    #[cfg(not(windows))]
    println!("system info:    only available on Windows");
    match qemu::find_adb() {
        Some(p) => println!("adb:            {}", p.display()),
        None => println!("adb:            NOT FOUND"),
    }
    let settings = Settings::load();
    println!(
        "ARM apps:       {}",
        if settings.arm_translation {
            "translation enabled (libhoudini / libndk in the guest)"
        } else {
            "translation disabled - ARM-only APKs will not start"
        }
    );
    if let Some(session) = crate::coordinator::read_live_session() {
        if let Ok(adb) = AdbClient::detect(session.adb_port) {
            if let Ok(status) = adb.shell("getprop ro.product.cpu.abilist") {
                println!("guest abilist:  {}", status.trim());
            }
        }
    }
    let registry = ImageRegistry::load();
    match registry.active_image() {
        Some(img) => println!(
            "android image:  {} ({}, {})",
            img.name,
            img.kind.as_str(),
            human_bytes(img.size_bytes)
        ),
        None => println!("android image:  none - `android-emu image add <file>`"),
    }
    println!("apps installed: {}", store::list().len());
    Ok(())
}

fn cmd_adb(args: Vec<String>) -> CmdResult {
    let Some(adb) = qemu::find_adb() else {
        return Err(fail("adb not found"));
    };
    let settings = Settings::load();
    let mut cmd = std::process::Command::new(adb);
    cmd.arg("-P").arg(settings.adb_port.to_string());
    cmd.args(&args);
    match cmd.status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(ExitCode(1)),
        Err(e) => Err(fail(e)),
    }
}

/// Wait until the VM is booted, used by `--install` in verbose mode.
pub fn wait_boot(port: u16, timeout_secs: u64) -> bool {
    let Ok(adb) = AdbClient::detect(port) else {
        return false;
    };
    adb.connect().is_ok() && adb.wait_for_boot(Duration::from_secs(timeout_secs), |msg| {
        let _ = writeln!(std::io::stderr(), "  {msg}");
    })
}
