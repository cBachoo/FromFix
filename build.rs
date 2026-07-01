use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Ship as a winmm.dll proxy. Rather than a second `.def` (which collides with
    // the one rustc generates for cdylibs), emit one `/EXPORT` linker directive
    // per real winmm export, each an absolute-path forwarder to the system winmm.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // The linker validates forwarder targets against an import library, so
        // winmm.lib must be on the link line or the winmm.* forwarders below
        // won't resolve (they'd fall back to looking for local symbols).
        println!("cargo:rustc-cdylib-link-arg=winmm.lib");

        let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
        let def_path = Path::new(&manifest).join("winmm.def");
        let text = fs::read_to_string(&def_path).expect("winmm.def missing");

        for line in text.lines() {
            let line = line.trim();
            // Lines look like: `name=C:\Windows\System32\winmm.name @ordinal`
            let Some((name, rest)) = line.split_once('=') else {
                continue;
            };
            let (target, ordinal) = match rest.split_once(" @") {
                Some((t, o)) => (t.trim(), Some(o.trim())),
                None => (rest.trim(), None),
            };
            let mut directive = format!("/EXPORT:{}={}", name.trim(), target);
            if let Some(ord) = ordinal {
                directive.push_str(&format!(",@{}", ord));
            }
            println!("cargo:rustc-cdylib-link-arg={}", directive);
        }
        println!("cargo:rerun-if-changed=winmm.def");
    }
}
