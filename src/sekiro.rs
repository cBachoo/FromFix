//! Pattern scans, byte patches and mid-function hooks. Each hook is an `ilhook`
//! jmp-back hook whose handler reads/writes the captured [`Registers`].

use crate::memory::{patch_bytes, pattern_scan};
use crate::state::{self, *};
use core::sync::atomic::Ordering::Relaxed;
use ilhook::x64::{CallbackOption, HookFlags, HookType, Hooker, JmpBackRoutine, Registers};

// ---- xmm helpers: the game keeps scalar floats in lane 0 (low 32 bits) ----
#[inline]
fn xmm_get(v: u128) -> f32 {
    f32::from_bits(v as u32)
}
#[inline]
fn xmm_set(v: &mut u128, f: f32) {
    *v = (*v & !0xFFFF_FFFFu128) | f.to_bits() as u128;
}

/// Install a jmp-back mid hook and leak the `HookPoint` so it stays active for
/// the life of the process (dropping it would unhook).
unsafe fn install_mid(name: &str, base: *const u8, addr: *mut u8, cb: JmpBackRoutine) {
    let off = addr as usize - base as usize;
    let hooker = Hooker::new(
        addr as usize,
        HookType::JmpBack(cb),
        CallbackOption::None,
        0,
        HookFlags::empty(),
    );
    match hooker.hook() {
        Ok(hp) => {
            core::mem::forget(hp);
            crate::log!("{}: hooked at exe+{:x}", name, off);
        }
        Err(e) => crate::log!("{}: hook FAILED at exe+{:x}: {:?}", name, off, e),
    }
}

/// The CurrentResolution signature, also used as the "code is ready" sentinel.
pub const READY_SIG: &str = "85 ?? 74 ?? ?? 8B ?? ?? ?? ?? ?? ?? 45 ?? ?? 74 ?? 41 ?? ?? 0F ?? ??";

/// Apply every Sekiro fix. Called after the code section has been decrypted.
pub unsafe fn apply(base: *const u8) {
    resolution(base);
    aspect_ratio(base);
    fov(base);
    hud(base);
    framerate(base);
}

// =====================================================================
// Resolution / borderless
// =====================================================================
unsafe fn resolution(base: *const u8) {
    match pattern_scan(base, READY_SIG) {
        Some(p) => install_mid("Current Resolution", base, p.add(0x2), h_current_resolution),
        None => crate::log!("Current Resolution: pattern scan failed."),
    }

    if UNLOCK_RES.load(Relaxed) {
        match pattern_scan(
            base,
            "0F 84 ?? ?? ?? ?? 48 8B ?? ?? ?? ?? ?? 48 85 ?? 0F 84 ?? ?? ?? ?? 0F ?? ?? ?? 48 8D ?? ?? E8 ?? ?? ?? ??",
        ) {
            Some(p) => {
                crate::log!("Resolution List: exe+{:x}", p as usize - base as usize);
                patch_bytes(p, &[0x90; 6]);
            }
            None => crate::log!("Resolution List: pattern scan failed."),
        }
    }

    if BORDERLESS.load(Relaxed) {
        match pattern_scan(base, "74 ?? 84 ?? B8 ?? ?? ?? ?? B9 ?? ?? ?? ?? 0F ?? ?? 48 83 ?? ?? C3") {
            Some(p) => {
                crate::log!("Windowed Mode Style: exe+{:x}", p as usize - base as usize);
                patch_bytes(p, &[0x90, 0x90]);
            }
            None => crate::log!("Windowed Mode Style: pattern scan failed."),
        }
    }
}

unsafe extern "win64" fn h_current_resolution(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    if FIX_ASPECT.load(Relaxed) {
        // Set ZF to jump over the forced scale-to-16:9.
        r.rflags |= 1 << 6;
    }
    let x = r.rax as i32;
    let y = r.r9 as i32;
    if RES_X.load(Relaxed) != x || RES_Y.load(Relaxed) != y {
        RES_X.store(x, Relaxed);
        RES_Y.store(y, Relaxed);
        state::calculate_aspect_ratio(true);
    }
}

// =====================================================================
// Aspect ratio
// =====================================================================
pub unsafe fn aspect_ratio(base: *const u8) {
    if !FIX_ASPECT.load(Relaxed) {
        return;
    }

    match pattern_scan(
        base,
        "F3 0F ?? ?? ?? BA 01 00 00 00 48 ?? ?? E8 ?? ?? ?? ?? 48 ?? ?? ?? 48 ?? ?? E8 ?? ?? ?? ??",
    ) {
        Some(p) => install_mid("Animation Culling Aspect", base, p, h_anim_cull),
        None => crate::log!("Animation Culling Aspect Ratio: pattern scan failed."),
    }

    let transition = pattern_scan(
        base,
        "F3 0F ?? ?? ?? ?? 0F ?? ?? 72 ?? 0F ?? ?? F3 0F ?? ?? F3 0F ?? ?? 0F ?? ?? 72 ??",
    );
    let culling = pattern_scan(
        base,
        "0F ?? ?? 0F ?? ?? ?? 72 ?? F3 0F ?? ?? ?? ?? ?? ?? 0F 57 ?? 0F ?? ?? 72 ??",
    );
    match (transition, culling) {
        (Some(t), Some(c)) => {
            install_mid("Awareness Markers Transition (Hor)", base, t, h_aware_trans_hor);
            install_mid("Awareness Markers Transition (Vert)", base, t.add(0x2A), h_aware_trans_vert);
            install_mid("Awareness Markers Culling", base, c, h_aware_cull);
        }
        _ => crate::log!("Awareness Markers: pattern scan(s) failed."),
    }
}

unsafe extern "win64" fn h_anim_cull(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    let ar = ASPECT_RATIO.get();
    if ar > NATIVE_ASPECT {
        xmm_set(&mut r.xmm3, ar);
    }
}

unsafe extern "win64" fn h_aware_trans_hor(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    let ar = ASPECT_RATIO.get();
    if ar > NATIVE_ASPECT {
        xmm_set(&mut r.xmm0, -((1080.0 * ar) - 1920.0) / 2.0);
        xmm_set(&mut r.xmm1, 1080.0 * ar);
    }
}

unsafe extern "win64" fn h_aware_trans_vert(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    let ar = ASPECT_RATIO.get();
    if ar < NATIVE_ASPECT {
        xmm_set(&mut r.xmm0, -((1920.0 / ar) - 1080.0) / 2.0);
        xmm_set(&mut r.xmm4, 1920.0 / ar);
    }
}

unsafe extern "win64" fn h_aware_cull(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    let ar = ASPECT_RATIO.get();
    if ar > NATIVE_ASPECT {
        let v = xmm_get(r.xmm2);
        xmm_set(&mut r.xmm2, v + ((1080.0 * ar) - 1920.0) / 2.0);
    } else if ar < NATIVE_ASPECT {
        let v = xmm_get(r.xmm3);
        xmm_set(&mut r.xmm3, v + ((1920.0 / ar) - 1080.0) / 2.0);
    }
}

// =====================================================================
// FOV
// =====================================================================
pub unsafe fn fov(base: *const u8) {
    if FOV_MULTI.get() == 1.0 {
        return;
    }
    match pattern_scan(
        base,
        "48 0F ?? ?? F3 0F ?? ?? F3 0F ?? ?? ?? ?? ?? ?? F3 0F ?? ?? ?? F3 41 ?? ?? ?? ?? ?? ?? ?? F3 0F ?? ??",
    ) {
        Some(p) => install_mid("Gameplay FOV", base, p.add(0x8), h_fov),
        None => crate::log!("Gameplay FOV: pattern scan failed."),
    }
}

unsafe extern "win64" fn h_fov(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    // Default FOV = 43
    let v = xmm_get(r.xmm1);
    xmm_set(&mut r.xmm1, v * FOV_MULTI.get());
}

// =====================================================================
// HUD (vignettes / fades)
// =====================================================================
pub unsafe fn hud(base: *const u8) {
    if !FIX_HUD.load(Relaxed) {
        return;
    }
    match pattern_scan(
        base,
        "8B ?? 0F ?? ?? F3 0F ?? ?? ?? F3 0F ?? ?? ?? F3 0F ?? ?? F3 0F ?? ?? ?? F3 0F ?? ?? ?? F3 0F ?? ?? 85 ??",
    ) {
        Some(p) => install_mid("HUD Scaleform GFX", base, p, h_hud),
        None => crate::log!("HUD: Scaleform GFX: pattern scan failed."),
    }
}

unsafe fn read_cstr(p: *const u8) -> String {
    let mut v = Vec::new();
    let mut i = 0usize;
    while i < 512 {
        let b = *p.add(i);
        if b == 0 {
            break;
        }
        v.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&v).into_owned()
}

unsafe extern "win64" fn h_hud(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    if r.rax == 0 {
        return;
    }
    let name_pp = (r.rax as usize + 0x48) as *const *const u8;
    let name_base = *name_pp;
    if name_base.is_null() {
        return;
    }
    let name = read_cstr(name_base.add(0xB));

    // Stretch screen vignetting effects / fades to fill the screen.
    if name.contains("01_201_stealtheffect.gfx")
        || name.contains("01_200_dyingeffect.gfx")
        || name.contains("01_910_fade.gfx")
        || name.contains("01_900_black.gfx")
    {
        let ar = ASPECT_RATIO.get();
        if ar > NATIVE_ASPECT {
            xmm_set(&mut r.xmm5, (RES_Y.load(Relaxed) as f32 * NATIVE_ASPECT) * 20.0);
        } else if ar < NATIVE_ASPECT {
            xmm_set(&mut r.xmm7, (RES_X.load(Relaxed) as f32 / NATIVE_ASPECT) * 20.0);
        }
    }
}

// =====================================================================
// Framerate
// =====================================================================
pub unsafe fn framerate(base: *const u8) {
    if !UNLOCK_FPS.load(Relaxed) {
        return;
    }

    let fs_rr = pattern_scan(base, "C7 ?? ?? 3C 00 00 00 48 ?? ?? ?? 01 00 00 00 4C ?? ?? ??");
    let fs_rr_startup = pattern_scan(base, "C7 45 ?? 3C 00 00 00 C7 45 ?? 01 00 00 00 48 8D ?? ??");
    match (fs_rr, fs_rr_startup) {
        (Some(a), Some(b)) => {
            crate::log!("Framerate: FS Refresh Rate: exe+{:x}", a as usize - base as usize);
            patch_bytes(a.add(0x3), &[0x00]);
            crate::log!("Framerate: FS Refresh Rate (Startup): exe+{:x}", b as usize - base as usize);
            patch_bytes(b.add(0x3), &[0x00]);
        }
        _ => crate::log!("Framerate: FS Refresh Rate: pattern scan(s) failed."),
    }

    match pattern_scan(base, "F3 0F ?? ?? ?? 0F ?? ?? 48 8B ?? ?? ?? ?? ?? 0F 57 ?? F2 ?? ?? ?? ??") {
        Some(p) => {
            crate::log!("Framerate: Cap: exe+{:x}", p as usize - base as usize);
            // movss xmm0,[rbx+18] -> xorps xmm0,xmm0
            patch_bytes(p, &[0x0F, 0x57, 0xC0, 0x90, 0x90]);
        }
        None => crate::log!("Framerate: Cap: pattern scan failed."),
    }

    match pattern_scan(base, "F3 0F ?? ?? ?? 4C ?? ?? ?? ?? 48 ?? ?? ?? ?? 0F ?? ?? F3 ?? ?? ?? ??") {
        Some(p) => install_mid("Current Framerate", base, p, h_current_framerate),
        None => crate::log!("Framerate: Current Framerate: pattern scan failed."),
    }

    match pattern_scan(
        base,
        "76 ?? F3 0F ?? ?? ?? ?? ?? ?? 0F ?? ?? ?? ?? ?? ?? 76 ?? F3 0F ?? ?? ?? ?? ?? ??",
    ) {
        Some(p) => install_mid("Sprint Speed", base, p, h_sprint_speed),
        None => crate::log!("Framerate: Sprint Speed: pattern scan failed."),
    }
}

unsafe extern "win64" fn h_current_framerate(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    let dt = xmm_get(r.xmm3);
    CUR_FPS.set(1.0 / dt);
}

unsafe extern "win64" fn h_sprint_speed(reg: *mut Registers, _: usize) {
    let r = &mut *reg;
    // Skip the movement-delta code path at >60fps so sprint speed isn't scaled down.
    if CUR_FPS.get() > 60.0 {
        r.rflags |= 1 << 6;
    }
}
