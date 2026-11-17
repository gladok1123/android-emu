//! android-emu - a lightweight Android emulator shell for Windows.
//!
//! The crate is intentionally dependency-free: ZIP/DEFLATE/AXML/ARSC/JSON
//! parsers and the PNG encoder all live inside `src/`.

// Public helpers are kept as a complete, documented API even where the CLI does
// not call them yet (library-style reuse, tests, future commands).
#![allow(dead_code)]

mod apk;
mod arsc;
mod axml;
mod cli;
mod config;
mod coordinator;
mod arm;
mod inflate;
mod installer;
mod json;
mod paths;
mod png;
mod qemu;
mod remote;
mod store;
mod util;
mod win;
mod zip;

/// Start the native UI, or focus it when it is already open.
fn gui_entry() -> i32 {
    #[cfg(windows)]
    {
        if win::gui::focus_existing() {
            return 0;
        }
        win::gui::run()
    }
    #[cfg(not(windows))]
    {
        println!("The graphical interface is only available on Windows.");
        println!("Use the command line instead, e.g.: android-emu --install app.apk");
        1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Shortcut/double-click launches should not show a console window; commands
    // typed in a terminal keep theirs (the console then has a parent process).
    let from_explorer = matches!(
        args.first().map(|a| a.as_str()),
        Some("--run") | Some("--install") | Some("phone")
    );
    if from_explorer {
        win::console::hide_console_if_owning();
    }
    // No arguments (or `gui`/`--ui`) opens the settings window.
    if args.is_empty() || matches!(args[0].as_str(), "gui" | "--gui" | "-g" | "--ui" | "settings") {
        std::process::exit(gui_entry());
    }
    let code = cli::run(args);
    std::process::exit(code);
}
