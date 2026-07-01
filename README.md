# FromFix

**FromFix** is a fix for **Sekiro: Shadows Die Twice** that can unlock the
framerate, adjust FOV, enable borderless windowed mode, and add ultrawide /
narrower aspect-ratio support.

It ships as a single `winmm.dll` proxy — drop it in the game folder and it loads
automatically. No ASI loader or other dependencies required.

## Features

### General
- Unlock the framerate (removes the 60 FPS cap).
- Adjust gameplay FOV.
- Borderless windowed mode.

### Ultrawide / Narrower
- Support for any aspect ratio (ultrawide **and** narrower than 16:9).
- Unlocked windowed resolution list.
- Fixed vignettes (low health / stealth).
- Fixed fades and animation culling at wider aspect ratios.

## Requirements
- 64-bit **Sekiro: Shadows Die Twice** (Steam).
- Windows 10/11, or Linux/Steam Deck via Proton (see below).

## Installation
1. Download the latest [release](../../../releases) (or build it yourself — see
   [Building](#building-from-source)).
2. Copy **`winmm.dll`** and **`FromFix.ini`** into the game folder — the one
   containing `sekiro.exe`.
   - Steam: right-click the game → **Manage → Browse local files**, or look in
     `steamapps\common\Sekiro`.
3. Launch the game normally. That's it.

> `winmm.dll` is a proxy: it forwards every real Windows `winmm` call on to the
> system `winmm.dll` and runs the fix on startup, so it works without an ASI
> loader.

### Steam Deck / Linux (Proton)
**Not needed on Windows.**

Add this to the game's launch options (right-click game → **Properties →
Launch Options**) so Proton loads the proxy DLL:

```
WINEDLLOVERRIDES="winmm=n,b" %command%
```

## Configuration
Edit **`FromFix.ini`** (in the game folder) with any text editor, then
restart the game. Settings:

| Section | Key | Default | Description |
|---|---|---|---|
| `Borderless Windowed` | `Enabled` | `false` | Run windowed mode as borderless. |
| `Gameplay FOV` | `Multiplier` | `1.00` | FOV scale. `>1` widens, `<1` narrows. Clamped to `0.01`–`4.00`. |
| `Unlock Framerate` | `Enabled` | `true` | Remove the 60 FPS cap. |
| `Unlock Resolutions` | `Enabled` | `true` | Unlock the windowed resolution list. |
| `Fix Aspect Ratio` | `Enabled` | `true` | Stop forced 16:9 scaling and fix related issues. |
| `Fix HUD` | `Enabled` | `true` | Fix vignettes/fades at non-16:9 resolutions. |

Set the in-game resolution to your monitor's native (ultrawide/narrow)
resolution after installing.

## Verifying it works
On launch, FromFix writes a **`FromFix.log`** next to `sekiro.exe`. Open it
to confirm each fix applied — you'll see the resolved address for every hook,
e.g. `Gameplay FOV: hooked at exe+…`, or a `pattern scan failed` line if a
signature didn't match (e.g. after a game update).

## Uninstalling
Delete `winmm.dll`, `FromFix.ini` and `FromFix.log` from the game folder.

## Building from source
Prerequisites:
- [Rust](https://rustup.rs/) with the MSVC toolchain
  (`rustup target add x86_64-pc-windows-msvc`, installed by default on Windows).
- Visual Studio Build Tools (MSVC linker + Windows SDK).

Build:
```sh
cargo build --release
```

The output is `target/release/winmm.dll` — copy it into the game folder next to
`FromFix.ini`. The static CRT is linked in, so there's no VC++ redist
dependency.

### How it's built
- Exports come from `winmm.def`; `build.rs` turns each into an absolute-path
  forwarder (`Name → C:\Windows\System32\winmm.Name`) and links `winmm.lib` so
  the linker can validate them. This keeps the proxy from importing itself.

### Layout
- `src/` — the Rust plugin: `memory` (pattern scan/patch), `state` (config +
  aspect math), `config` (ini parser), `fixes` (the hooks), `logger`, `lib`
  (`DllMain`).
- `vendor/ilhook/` — a vendored fork of [ilhook](https://github.com/regomne/ilhook-rs)
  patched to expose `xmm0`–`xmm15` in its `Registers` (upstream only exposes
  `xmm0`–`xmm3`, which the HUD and narrower-than-16:9 fixes need).

## Credits
- [ilhook](https://github.com/regomne/ilhook-rs) — the inline-hook engine
  (vendored + patched here to expose all xmm registers).

## Troubleshooting
- **Game won't start / instantly closes:** make sure you're using the 64-bit
  build and that `winmm.dll` sits next to `sekiro.exe`. Remove it to confirm the
  game runs vanilla.
- **No effect in-game:** check `FromFix.log`. `pattern scan failed` lines mean
  the signatures need updating for your game version.
- **Ultrawide still pillarboxed:** ensure `Fix Aspect Ratio` is `Enabled` and
  the in-game resolution matches your display.
