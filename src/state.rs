//! Configuration flags and runtime aspect-ratio/HUD state.
//!
//! Everything lives in atomics because the config is written once by the loader
//! thread at startup while the hook handlers read (and update) it from the
//! game's own threads.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

const O: Ordering = Ordering::Relaxed;

/// 16:9, the aspect ratio the game targets natively.
pub const NATIVE_ASPECT: f32 = 16.0 / 9.0;

/// An `f32` stored inside an `AtomicU32` (as its bit pattern).
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub const fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(O))
    }
    #[inline]
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), O);
    }
}

// ---- Config (from FromFix.ini) ----
pub static BORDERLESS: AtomicBool = AtomicBool::new(false);
pub static UNLOCK_FPS: AtomicBool = AtomicBool::new(false);
pub static UNLOCK_RES: AtomicBool = AtomicBool::new(false);
pub static FIX_ASPECT: AtomicBool = AtomicBool::new(false);
pub static FIX_HUD: AtomicBool = AtomicBool::new(false);
pub static FOV_MULTI: AtomicF32 = AtomicF32::new(1.0);

// ---- Runtime state ----
pub static RES_X: AtomicI32 = AtomicI32::new(0);
pub static RES_Y: AtomicI32 = AtomicI32::new(0);
pub static ASPECT_RATIO: AtomicF32 = AtomicF32::new(0.0);
pub static ASPECT_MULT: AtomicF32 = AtomicF32::new(0.0);
pub static HUD_WIDTH: AtomicF32 = AtomicF32::new(0.0);
pub static HUD_HEIGHT: AtomicF32 = AtomicF32::new(0.0);
pub static HUD_WIDTH_OFF: AtomicF32 = AtomicF32::new(0.0);
pub static HUD_HEIGHT_OFF: AtomicF32 = AtomicF32::new(0.0);
pub static CUR_FPS: AtomicF32 = AtomicF32::new(0.0);

/// Recompute aspect ratio and HUD placement from the current resolution.
/// Direct port of `CalculateAspectRatio` from the original fix.
pub fn calculate_aspect_ratio(do_log: bool) {
    let x = RES_X.load(O);
    let y = RES_Y.load(O);
    if x <= 0 || y <= 0 {
        return;
    }
    let (xf, yf) = (x as f32, y as f32);

    let aspect = xf / yf;
    ASPECT_RATIO.set(aspect);
    ASPECT_MULT.set(aspect / NATIVE_ASPECT);

    // Pillarboxed (wider than 16:9) by default.
    let mut hud_w = yf * NATIVE_ASPECT;
    let mut hud_h = yf;
    let mut hud_wo = (xf - yf * NATIVE_ASPECT) / 2.0;
    let mut hud_ho = 0.0f32;
    if aspect < NATIVE_ASPECT {
        // Letterboxed (narrower than 16:9).
        hud_w = xf;
        hud_h = xf / NATIVE_ASPECT;
        hud_wo = 0.0;
        hud_ho = (yf - xf / NATIVE_ASPECT) / 2.0;
    }
    HUD_WIDTH.set(hud_w);
    HUD_HEIGHT.set(hud_h);
    HUD_WIDTH_OFF.set(hud_wo);
    HUD_HEIGHT_OFF.set(hud_ho);

    if do_log {
        crate::log!("----------");
        crate::log!("Current Resolution: {}x{}", x, y);
        crate::log!("fAspectRatio: {}", aspect);
        crate::log!("fAspectMultiplier: {}", aspect / NATIVE_ASPECT);
        crate::log!("fHUDWidth: {}", hud_w);
        crate::log!("fHUDHeight: {}", hud_h);
        crate::log!("fHUDWidthOffset: {}", hud_wo);
        crate::log!("fHUDHeightOffset: {}", hud_ho);
        crate::log!("----------");
    }
}
