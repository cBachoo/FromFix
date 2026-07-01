//! Pattern scanning and byte patching over the loaded game module.
//! Direct port of the `Memory` helpers from the original `helper.hpp`.

use core::ffi::c_void;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE};

/// Read `SizeOfImage` straight out of the PE optional header.
pub unsafe fn size_of_image(base: *const u8) -> usize {
    let e_lfanew = *(base.add(0x3C) as *const i32) as usize;
    let nt = base.add(e_lfanew);
    // OptionalHeader starts at nt+0x18; SizeOfImage is at +0x38 within it (PE32+).
    *(nt.add(0x50) as *const u32) as usize
}

/// Read `FileHeader.TimeDateStamp` (used only for logging).
pub unsafe fn module_timestamp(base: *const u8) -> u32 {
    let e_lfanew = *(base.add(0x3C) as *const i32) as usize;
    let nt = base.add(e_lfanew);
    *(nt.add(8) as *const u32)
}

/// Turn an IDA-style signature (`"85 ?? 74 ?? ??"`) into a byte/wildcard list.
/// `None` entries are wildcards.
fn parse_pattern(sig: &str) -> Vec<Option<u8>> {
    sig.split_whitespace()
        .map(|t| {
            if t.contains('?') {
                None
            } else {
                u8::from_str_radix(t, 16).ok()
            }
        })
        .collect()
}

/// Scan the whole image for the first match of `sig`. Returns its address.
pub unsafe fn pattern_scan(base: *const u8, sig: &str) -> Option<*mut u8> {
    let size = size_of_image(base);
    let pat = parse_pattern(sig);
    let n = pat.len();
    if n == 0 || size < n {
        return None;
    }
    let data = core::slice::from_raw_parts(base, size);
    'outer: for i in 0..=size - n {
        for (j, want) in pat.iter().enumerate() {
            if let Some(b) = want {
                if data[i + j] != *b {
                    continue 'outer;
                }
            }
        }
        return Some(base.add(i) as *mut u8);
    }
    None
}

/// Find every occurrence of a literal byte sequence in the image.
pub unsafe fn find_all(base: *const u8, needle: &[u8]) -> Vec<*mut u8> {
    let size = size_of_image(base);
    let n = needle.len();
    let mut out = Vec::new();
    if n == 0 || size < n {
        return out;
    }
    let data = core::slice::from_raw_parts(base, size);
    let mut i = 0;
    while i + n <= size {
        if &data[i..i + n] == needle {
            out.push(base.add(i) as *mut u8);
            i += n;
        } else {
            i += 1;
        }
    }
    out
}

/// Poll for `sig` until it appears. FromSoftware exes are SteamStub-encrypted;
/// `.text` is only decrypted once the real entry point runs, which is *after*
/// statically-imported DLLs (like this winmm proxy) initialise. Returns `true`
/// once found, `false` on timeout. `label` is used for logging.
pub unsafe fn wait_for_signature(base: *const u8, sig: &str, label: &str) -> bool {
    use windows_sys::Win32::System::Threading::Sleep;
    const TIMEOUT_MS: u32 = 60_000;
    const STEP_MS: u32 = 250;
    let mut waited = 0u32;
    loop {
        if pattern_scan(base, sig).is_some() {
            crate::log!("{}: game code decrypted after {} ms.", label, waited);
            return true;
        }
        if waited >= TIMEOUT_MS {
            crate::log!("{}: WARNING code not ready after {} ms; scanning anyway.", label, waited);
            return false;
        }
        Sleep(STEP_MS);
        waited += STEP_MS;
    }
}

/// Overwrite `bytes.len()` bytes at `addr`, restoring the original protection.
pub unsafe fn patch_bytes(addr: *mut u8, bytes: &[u8]) {
    let mut old: u32 = 0;
    VirtualProtect(
        addr as *const c_void,
        bytes.len(),
        PAGE_EXECUTE_READWRITE,
        &mut old,
    );
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr, bytes.len());
    VirtualProtect(addr as *const c_void, bytes.len(), old, &mut old);
}
