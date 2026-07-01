//! FromFix — a pure-Rust ASI plugin for *Sekiro: Shadows Die Twice*.
//! Rewrite of the original C++ fix (unlock framerate, FOV, borderless,
//! ultrawide/narrower aspect-ratio + HUD support).

#![allow(non_snake_case)]

#[macro_use]
mod logger;
mod config;
mod fixes;
mod memory;
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

const VERSION: &str = "0.1.0";
const DLL_PROCESS_ATTACH: u32 = 1;

static THIS_MODULE: AtomicUsize = AtomicUsize::new(0);

/// Return the directory (with trailing separator) that `module` was loaded from.
unsafe fn module_dir(module: HMODULE) -> String {
    let mut buf = [0u16; 260];
    let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
    let s = String::from_utf16_lossy(&buf[..n]);
    match s.rfind('\\') {
        Some(i) => s[..=i].to_string(),
        None => s,
    }
}

/// The actual worker: log, parse config, apply every fix.
unsafe fn run() {
    let exe = GetModuleHandleW(core::ptr::null());
    let exe_base = exe as *const u8;
    let exe_dir = module_dir(exe);
    let dll_dir = module_dir(THIS_MODULE.load(Ordering::Relaxed) as HMODULE);

    logger::init(&format!("{}FromFix.log", exe_dir));
    crate::log!("----------");
    crate::log!("FromFix v{} loaded.", VERSION);
    crate::log!("Module base: 0x{:x}", exe_base as usize);
    crate::log!("Module timestamp: {}", memory::module_timestamp(exe_base));
    crate::log!("----------");

    let ini = format!("{}FromFix.ini", dll_dir);
    if !config::load(&ini) {
        crate::log!("ERROR: could not locate config file: {}", ini);
        return;
    }

    // sekiro.exe is SteamStub-encrypted; wait until the real entry point has
    // decrypted the code section before scanning for signatures.
    fixes::wait_until_ready(exe_base);

    fixes::resolution(exe_base);
    fixes::aspect_ratio(exe_base);
    fixes::fov(exe_base);
    fixes::hud(exe_base);
    fixes::framerate(exe_base);
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
