//! Dark Souls III fixes.
//!
//! Ported to our in-process winmm-proxy framework from the community tools:
//! - FPS unlock (debug frame-limiter): 0dm/saucy — <https://github.com/0dm/DS3DebugFPS>
//! - No-Intro: bladecoding — <https://github.com/bladecoding/DarkSouls3RemoveIntroScreens>

use crate::memory::{patch_bytes, scan_retry as pattern_scan};
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

/// DS3's selectable resolutions are a fixed data table (unlike Sekiro's
/// runtime-filtered list, so there is no conditional to "unlock"). To make your
/// resolution appear in the in-game list we overwrite the standard 1920x1080
/// entry with the target resolution. Entry layout is width-u32-LE + height-u16-LE
/// (e.g. 1920x1080 = `80 07 00 00 38 04`).
const RES_LIST_ENTRY_1920X1080: [u8; 6] = [0x80, 0x07, 0x00, 0x00, 0x38, 0x04];

/// If borderless is on but no explicit DS3 resolution is set, default the added
/// resolution to the primary monitor so a borderless (full-monitor) window
/// matches the render 1:1 — no stretch. Must run before `patch_resolution`.
pub unsafe fn default_resolution_to_monitor() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    if DS3_BORDERLESS.load(Relaxed) && DS3_RES_WIDTH.load(Relaxed) <= 0 {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w > 0 && h > 0 {
            DS3_RES_WIDTH.store(w, Relaxed);
            DS3_RES_HEIGHT.store(h, Relaxed);
            crate::log!("DS3: borderless with no set resolution -> adding monitor {}x{} to the list.", w, h);
        }
    }
}

/// DS3 clamps the *windowed* resolution and won't persist an ultrawide one
/// (it keeps a separate, clamped `Resolution-WindowScreen*` in GraphicsConfig.xml).
/// When borderless is on, write the target resolution straight into that config
/// (which the game reads at startup) so windowed renders at the full monitor and
/// the setting sticks across restarts.
pub unsafe fn force_windowed_config() {
    if !DS3_BORDERLESS.load(Relaxed) {
        return;
    }
    let w = DS3_RES_WIDTH.load(Relaxed);
    let h = DS3_RES_HEIGHT.load(Relaxed);
    if w <= 0 || h <= 0 {
        return;
    }
    let appdata = match std::env::var("APPDATA") {
        Ok(v) => v,
        Err(_) => return,
    };
    let path = format!("{}\\DarkSoulsIII\\GraphicsConfig.xml", appdata);
    let raw = match std::fs::read(&path) {
        Ok(r) => r,
        Err(_) => {
            crate::log!("DS3 Config: GraphicsConfig.xml not found ({}).", path);
            return;
        }
    };
    let xml = decode_utf16le(&raw);
    let mut out = set_xml_tag(&xml, "ScreenMode", "WINDOW");
    out = set_xml_tag(&out, "Resolution-WindowScreenWidth", &w.to_string());
    out = set_xml_tag(&out, "Resolution-WindowScreenHeight", &h.to_string());
    if out == xml {
        crate::log!("DS3 Config: GraphicsConfig.xml already {}x{} windowed.", w, h);
        return;
    }
    match std::fs::write(&path, encode_utf16le(&out)) {
        Ok(_) => crate::log!("DS3 Config: set windowed {}x{} in GraphicsConfig.xml.", w, h),
        Err(e) => crate::log!("DS3 Config: write failed: {:?}", e),
    }
}

fn decode_utf16le(raw: &[u8]) -> String {
    let start = if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE { 2 } else { 0 };
    let units: Vec<u16> = raw[start..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

fn encode_utf16le(s: &str) -> Vec<u8> {
    let mut b = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        b.extend_from_slice(&u.to_le_bytes());
    }
    b
}

/// Replace the inner text of `<tag>...</tag>` with `val` (first occurrence).
fn set_xml_tag(xml: &str, tag: &str, val: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let (Some(s), Some(e)) = (xml.find(&open), xml.find(&close)) {
        let start = s + open.len();
        if start <= e {
            return format!("{}{}{}", &xml[..start], val, &xml[e..]);
        }
    }
    xml.to_string()
}

/// Replace the 1920x1080 resolution-list entry with the configured resolution,
/// so it becomes selectable in the in-game Settings (works in windowed too).
/// Targets an unencrypted data table, so it runs before the decryption wait.
pub unsafe fn patch_resolution(base: *const u8) {
    let w = DS3_RES_WIDTH.load(Relaxed);
    let h = DS3_RES_HEIGHT.load(Relaxed);
    if w <= 0 || h <= 0 {
        return;
    }
    let mut to = Vec::with_capacity(6);
    to.extend_from_slice(&(w as u32).to_le_bytes());
    to.extend_from_slice(&(h as u16).to_le_bytes());

    let hits = crate::memory::find_all(base, &RES_LIST_ENTRY_1920X1080);
    if hits.is_empty() {
        crate::log!("DS3 Resolution: 1920x1080 list entry not found (nothing changed).");
        return;
    }
    for p in &hits {
        patch_bytes(*p, &to);
    }
    crate::log!(
        "DS3 Resolution: replaced {} entry(s) 1920x1080 -> {}x{} (pick it in-game).",
        hits.len(),
        w,
        h
    );
}

/// Apply the code-based Dark Souls III fixes. Called after `.text` is decrypted.
pub unsafe fn apply(base: *const u8) {
    skip_intro(base);
    framerate(base);
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

/// Keep the game window a full-monitor borderless popup — the same idea as
/// Sekiro's borderless. Run the game **in-game Windowed** and this turns it into
/// borderless fullscreen: no exclusive fullscreen, so alt-tab is instant and
/// never minimises, and the taskbar is hidden.
///
/// This *persistently* re-applies (only while the game is focused, so it doesn't
/// fight alt-tab), because DS3 re-creates its window whenever you change the
/// resolution in-game — a one-shot would be undone by that.
pub unsafe fn borderless() {
    if !DS3_BORDERLESS.load(Relaxed) {
        return;
    }
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::System::Threading::Sleep;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowA, GetForegroundWindow, GetSystemMetrics, GetWindowLongA, GetWindowRect, IsWindow,
        SetWindowLongA, SetWindowPos, GWL_STYLE, HWND_TOP, SM_CXSCREEN, SM_CYSCREEN, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_SHOWWINDOW, WS_CAPTION, WS_POPUP, WS_THICKFRAME, WS_VISIBLE,
    };

    let title = b"DARK SOULS III\0";
    let mut hwnd: HWND = core::ptr::null_mut();
    for _ in 0..200 {
        hwnd = FindWindowA(core::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            break;
        }
        Sleep(100);
    }
    if hwnd.is_null() {
        crate::log!("DS3 Borderless: game window not found.");
        return;
    }

    let dw = GetSystemMetrics(SM_CXSCREEN);
    let dh = GetSystemMetrics(SM_CYSCREEN);
    let border_mask = (WS_CAPTION | WS_THICKFRAME) as i32;
    crate::log!("DS3 Borderless: enforcing full-screen borderless at {}x{}.", dw, dh);

    loop {
        if IsWindow(hwnd) == 0 {
            break; // game closed
        }
        // Only touch the window while the game is focused, so we don't yank it
        // back over other apps when the user alt-tabs away.
        if GetForegroundWindow() == hwnd {
            let style = GetWindowLongA(hwnd, GWL_STYLE);
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let got = GetWindowRect(hwnd, &mut r) != 0;
            let bordered = (style & border_mask) != 0;
            let full = got && r.left == 0 && r.top == 0 && r.right == dw && r.bottom == dh;
            if bordered || !full {
                SetWindowLongA(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as i32);
                SetWindowPos(
                    hwnd,
                    HWND_TOP,
                    0,
                    0,
                    dw,
                    dh,
                    SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
                );
            }
        }
        Sleep(300);
    }
}

