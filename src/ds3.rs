//! Dark Souls III fixes.
//!
//! Ported to our in-process winmm-proxy framework from the community tools:
//! - FPS unlock (debug frame-limiter): 0dm/saucy — <https://github.com/0dm/DS3DebugFPS>
//! - No-Intro: bladecoding — <https://github.com/bladecoding/DarkSouls3RemoveIntroScreens>

use crate::memory::{pattern_scan, patch_bytes};
use crate::state::*;
use core::sync::atomic::Ordering::Relaxed;

/// Version-independent signature for the intro-screen load call. Also used as the
/// "code is decrypted / ready to scan" sentinel. The two `E8 rel32` calls are
/// wildcarded so it matches every DS3 version.
pub const READY_SIG: &str =
    "E8 ?? ?? ?? ?? 90 4D 8B C7 49 8B D4 48 8B C8 E8 ?? ?? ?? ??";

/// Patch that replaces the intro-load calls: `xor rax,rax; mov [rdx],rax;
/// mov [r12],rax;` then NOPs — i.e. skip loading the logo movies.
const SKIP_INTRO_PATCH: [u8; 20] = [
    0x48, 0x31, 0xC0, 0x48, 0x89, 0x02, 0x49, 0x89, 0x04, 0x24, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
    0x90, 0x90, 0x90, 0x90,
];

/// The two SprjFlipper global-pointer signatures (the frame-limiter singleton).
/// Bytes 2-3 are wildcarded; the value read at the match is the pointer itself.
const SPRJFLIPPER_SIGS: [&str; 2] = [
    "50 60 ?? ?? F4 7F 00 00 00 00 00 00 00 00 00 00",
    "50 60 ?? ?? F3 7F 00 00 00 00 00 00 00 00 00 00",
];
const FLIPPER_FPS_OFFSET: usize = 0x354; // float: target frame rate
const FLIPPER_DEBUG_OFFSET: usize = 0x358; // u8: enable the debug limiter

/// Patch the hardcoded internal resolution reference (1280x720) to the
/// configured resolution, enabling native ultrawide / arbitrary resolutions.
///
/// This targets an unencrypted data reference, so it runs *before* the
/// SteamStub decryption wait — as early as possible, to beat the game reading
/// it during startup. The 6-byte layout is width-u32-LE followed by height-u16-LE.
pub unsafe fn patch_resolution(base: *const u8) {
    let w = DS3_RES_WIDTH.load(Relaxed);
    let h = DS3_RES_HEIGHT.load(Relaxed);
    if w <= 0 || h <= 0 {
        return;
    }
    let mut from = Vec::with_capacity(6);
    from.extend_from_slice(&1280u32.to_le_bytes());
    from.extend_from_slice(&720u16.to_le_bytes());
    let mut to = Vec::with_capacity(6);
    to.extend_from_slice(&(w as u32).to_le_bytes());
    to.extend_from_slice(&(h as u16).to_le_bytes());

    let hits = crate::memory::find_all(base, &from);
    if hits.is_empty() {
        crate::log!("DS3 Resolution: 1280x720 reference not found (nothing patched).");
        return;
    }
    for p in &hits {
        patch_bytes(*p, &to);
    }
    crate::log!("DS3 Resolution: patched {} reference(s) 1280x720 -> {}x{}.", hits.len(), w, h);
}

/// Apply every Dark Souls III fix. Called after the code section is decrypted.
pub unsafe fn apply(base: *const u8) {
    skip_intro(base);
    framerate(base);
    borderless();
}

fn skip_intro(base: *const u8) {
    if !DS3_SKIP_INTRO.load(Relaxed) {
        return;
    }
    unsafe {
        match pattern_scan(base, READY_SIG) {
            Some(p) => {
                crate::log!("DS3 Skip Intro: patching at exe+{:x}", p as usize - base as usize);
                patch_bytes(p, &SKIP_INTRO_PATCH);
            }
            None => crate::log!("DS3 Skip Intro: pattern scan failed."),
        }
    }
}

fn framerate(base: *const u8) {
    if !DS3_UNLOCK_FPS.load(Relaxed) {
        return;
    }
    let target = DS3_TARGET_FPS.get();
    unsafe {
        for sig in SPRJFLIPPER_SIGS {
            let Some(p) = pattern_scan(base, sig) else {
                continue;
            };
            // The 8 bytes at the match are a pointer to the SprjFlipper singleton.
            let flipper = *(p as *const u64) as usize;
            if flipper == 0 {
                continue;
            }
            crate::log!(
                "DS3 Unlock Framerate: SprjFlipper @ 0x{:x} -> {} FPS",
                flipper,
                target
            );
            patch_bytes((flipper + FLIPPER_FPS_OFFSET) as *mut u8, &target.to_le_bytes());
            patch_bytes((flipper + FLIPPER_DEBUG_OFFSET) as *mut u8, &[1u8]);
            return;
        }
        crate::log!("DS3 Unlock Framerate: SprjFlipper pattern not found.");
    }
}

fn borderless() {
    if !DS3_BORDERLESS.load(Relaxed) {
        return;
    }
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::System::Threading::Sleep;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AdjustWindowRect, FindWindowA, GetSystemMetrics, GetWindowLongA, MoveWindow, SetWindowLongA,
        GWL_EXSTYLE, GWL_STYLE, SM_CXSCREEN, SM_CYSCREEN, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    unsafe {
        // The window may not exist the instant we run; poll briefly.
        let title = b"DARK SOULS III\0";
        let mut hwnd: HWND = core::ptr::null_mut();
        for _ in 0..40 {
            hwnd = FindWindowA(core::ptr::null(), title.as_ptr());
            if !hwnd.is_null() {
                break;
            }
            Sleep(250);
        }
        if hwnd.is_null() {
            crate::log!("DS3 Borderless: game window not found.");
            return;
        }

        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        let mut rect = RECT { left: 0, top: 0, right: w, bottom: h };

        SetWindowLongA(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as i32);
        AdjustWindowRect(&mut rect, GetWindowLongA(hwnd, GWL_STYLE) as u32, 0);
        SetWindowLongA(hwnd, GWL_EXSTYLE, GetWindowLongA(hwnd, GWL_EXSTYLE) | WS_EX_TOPMOST as i32);
        MoveWindow(hwnd, 0, 0, rect.right - rect.left, rect.bottom - rect.top, 1);
        crate::log!("DS3 Borderless: applied at {}x{}.", w, h);
    }
}
