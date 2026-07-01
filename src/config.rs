//! Parse `FromFix.ini` into the global config atomics.

use crate::state::*;
use core::sync::atomic::Ordering::Relaxed;
use std::collections::HashMap;

/// Load and apply the ini file. Returns `false` if the file can't be read.
pub fn load(path: &str) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return false,
    };

    let mut section = String::new();
    let mut map: HashMap<(String, String), String> = HashMap::new();
    for raw in text.lines() {
        // Strip trailing comments.
        let line = match raw.find(';') {
            Some(i) => &raw[..i],
            None => raw,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            continue;
        }
        if let Some(eq) = line.find('=') {
            let k = line[..eq].trim().to_string();
            let v = line[eq + 1..].trim().to_string();
            map.insert((section.clone(), k), v);
        }
    }

    let get = |s: &str, k: &str| map.get(&(s.to_string(), k.to_string())).cloned();
    let get_bool = |s: &str, k: &str, d: bool| {
        get(s, k)
            .map(|v| {
                let v = v.to_ascii_lowercase();
                v == "true" || v == "1" || v == "yes" || v == "on"
            })
            .unwrap_or(d)
    };
    let get_f32 = |s: &str, k: &str, d: f32| get(s, k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    let get_i32 = |s: &str, k: &str, d: i32| get(s, k).and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(d);

    // Sekiro
    BORDERLESS.store(get_bool("Borderless Windowed", "Enabled", false), Relaxed);
    UNLOCK_FPS.store(get_bool("Unlock Framerate", "Enabled", false), Relaxed);
    UNLOCK_RES.store(get_bool("Unlock Resolutions", "Enabled", false), Relaxed);
    FIX_ASPECT.store(get_bool("Fix Aspect Ratio", "Enabled", false), Relaxed);
    FIX_HUD.store(get_bool("Fix HUD", "Enabled", false), Relaxed);
    FOV_MULTI.set(get_f32("Gameplay FOV", "Multiplier", 1.0).clamp(0.01, 4.0));

    // Dark Souls III
    DS3_SKIP_INTRO.store(get_bool("DS3 Skip Intro", "Enabled", true), Relaxed);
    DS3_UNLOCK_FPS.store(get_bool("DS3 Unlock Framerate", "Enabled", false), Relaxed);
    DS3_TARGET_FPS.set(get_f32("DS3 Unlock Framerate", "TargetFPS", 60.0).clamp(30.0, 1000.0));
    DS3_BORDERLESS.store(get_bool("DS3 Borderless", "Enabled", false), Relaxed);
    DS3_RES_WIDTH.store(get_i32("DS3 Resolution", "Width", 0), Relaxed);
    DS3_RES_HEIGHT.store(get_i32("DS3 Resolution", "Height", 0), Relaxed);

    crate::log!("----------");
    crate::log!("[Sekiro] Borderless Windowed = {}", BORDERLESS.load(Relaxed));
    crate::log!("[Sekiro] Gameplay FOV Multiplier = {}", FOV_MULTI.get());
    crate::log!("[Sekiro] Unlock Framerate = {}", UNLOCK_FPS.load(Relaxed));
    crate::log!("[Sekiro] Unlock Resolutions = {}", UNLOCK_RES.load(Relaxed));
    crate::log!("[Sekiro] Fix Aspect Ratio = {}", FIX_ASPECT.load(Relaxed));
    crate::log!("[Sekiro] Fix HUD = {}", FIX_HUD.load(Relaxed));
    crate::log!("[DS3] Skip Intro = {}", DS3_SKIP_INTRO.load(Relaxed));
    crate::log!("[DS3] Unlock Framerate = {} (TargetFPS {})", DS3_UNLOCK_FPS.load(Relaxed), DS3_TARGET_FPS.get());
    crate::log!("[DS3] Borderless = {}", DS3_BORDERLESS.load(Relaxed));
    crate::log!("[DS3] Resolution = {}x{}", DS3_RES_WIDTH.load(Relaxed), DS3_RES_HEIGHT.load(Relaxed));
    crate::log!("----------");

    true
}
