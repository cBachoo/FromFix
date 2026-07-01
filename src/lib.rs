//! FromFix — a pure-Rust winmm.dll proxy that fixes FromSoftware games.
//! One DLL, multiple games: it detects the host executable and applies the
//! matching fix set (Sekiro, Dark Souls III).

#![allow(non_snake_case)]

#[macro_use]
mod logger;
mod config;
mod ds3;
mod memory;
mod sekiro;
mod state;

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    DisableThreadLibraryCalls, GetModuleFileNameW, GetModuleHandleW,
};
use windows_sys::Win32::System::Threading::{
    CreateThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DLL_PROCESS_ATTACH: u32 = 1;

static THIS_MODULE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, PartialEq)]
enum Game {
    Sekiro,
    DarkSouls3,
    Unknown,
}

/// Full path of `module` (empty on failure).
unsafe fn module_path(module: HMODULE) -> String {
    let mut buf = [0u16; 260];
    let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
    String::from_utf16_lossy(&buf[..n])
}

/// Directory (with trailing separator) of a module path string.
fn dir_of(path: &str) -> String {
    match path.rfind(['\\', '/']) {
        Some(i) => path[..=i].to_string(),
        None => String::new(),
    }
}

/// Lowercase file name of a path string.
fn file_of(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase()
}

/// The actual worker: detect the game, parse config, apply the right fixes.
unsafe fn run() {
    let exe = GetModuleHandleW(core::ptr::null());
    let exe_base = exe as *const u8;
    let exe_path = module_path(exe);
    let exe_name = file_of(&exe_path);
    let exe_dir = dir_of(&exe_path);
    let dll_dir = dir_of(&module_path(THIS_MODULE.load(Ordering::Relaxed) as HMODULE));

    logger::init(&format!("{}FromFix.log", exe_dir));
    crate::log!("----------");
    crate::log!("FromFix v{} loaded.", VERSION);
    crate::log!("Host executable: {}", exe_name);
    crate::log!("Module base: 0x{:x}", exe_base as usize);
    crate::log!("Module timestamp: {}", memory::module_timestamp(exe_base));
    crate::log!("----------");

    let game = match exe_name.as_str() {
        "sekiro.exe" => Game::Sekiro,
        "darksoulsiii.exe" => Game::DarkSouls3,
        _ => Game::Unknown,
    };
    if game == Game::Unknown {
        crate::log!("Unsupported host executable '{}'; nothing to do.", exe_name);
        return;
    }

    let ini = format!("{}FromFix.ini", dll_dir);
    if !config::load(&ini) {
        crate::log!("ERROR: could not locate config file: {}", ini);
        return;
    }

    // FromSoftware exes are SteamStub-encrypted; wait until the real entry point
    // has decrypted .text before scanning for signatures.
    match game {
        Game::Sekiro => {
            memory::wait_for_signature(exe_base, sekiro::READY_SIG, "Sekiro");
            sekiro::apply(exe_base);
        }
        Game::DarkSouls3 => {
            // Resolution is an unencrypted data reference — patch it ASAP, before
            // waiting on code decryption, to beat the game's startup read.
            ds3::patch_resolution(exe_base);
            memory::wait_for_signature(exe_base, ds3::READY_SIG, "DS3");
            ds3::apply(exe_base);
        }
        Game::Unknown => unreachable!(),
    }
    crate::log!("Initialisation complete.");
}

unsafe extern "system" fn main_thread(_: *mut c_void) -> u32 {
    run();
    0
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(hinst: HMODULE, reason: u32, _reserved: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        THIS_MODULE.store(hinst as usize, Ordering::Relaxed);
        DisableThreadLibraryCalls(hinst);
        let h: HANDLE = CreateThread(
            core::ptr::null(),
            0,
            Some(main_thread),
            core::ptr::null(),
            0,
            core::ptr::null_mut(),
        );
        if !h.is_null() {
            SetThreadPriority(h, THREAD_PRIORITY_HIGHEST);
            CloseHandle(h);
        }
    }
    1
}
