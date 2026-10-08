// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The system allocator on macOS, mimalloc on Linux or with the `mimalloc` feature.
// `cmdr-fs` decides, so the memory readers always know which heap they're reading.
#[global_allocator]
static GLOBAL: cmdr_fs::process_memory::GlobalAlloc = cmdr_fs::process_memory::GLOBAL_ALLOC;

fn main() {
    cmdr_lib::run()
}
