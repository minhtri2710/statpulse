// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if superfan_lib::mcp::maybe_handle_cli_args() {
        return;
    }
    superfan_lib::run()
}
