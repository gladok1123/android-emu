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
    if args.is_empty() {
        cli::print_help_public();
        return;
    }
    let code = cli::run(args);
    std::process::exit(code);
}
