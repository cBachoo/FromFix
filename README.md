# FromFix

**FromFix** is an all-in-one, pure-Rust fix for FromSoftware PC games. It ships
as a single `winmm.dll` proxy that auto-detects the game it's loaded into and
applies the matching fixes — drop it in the game folder and it just works. 

**Supported games:** Sekiro: Shadows Die Twice · Dark Souls III

## Features

### Sekiro: Shadows Die Twice
- Unlock the framerate (removes the 60 FPS cap).
- Adjust gameplay FOV.
- Borderless windowed mode.
- Ultrawide / narrower: any aspect ratio, unlocked resolution list, fixed
  vignettes/fades and animation culling.

### Dark Souls III
- Skip the startup logo/intro movies.
- Unlock the framerate (debug frame limiter; see the note below).
- Borderless fullscreen.
- Ultrawide / native resolution (set it in `FromFix.ini`).

> **DS3 framerate note:** Dark Souls III ties physics to framerate, so running
> above 60 FPS can cause quirks (fall damage, item pickups). `TargetFPS` is
> yours to choose; 60 is the safe default.

## Installation
1. Grab the latest [release](../../../releases) (or build it — see below).
2. Copy **`winmm.dll`** and **`FromFix.ini`** into the game folder:
   - **Sekiro:** next to `sekiro.exe` (e.g. `steamapps\common\Sekiro`).
   - **Dark Souls III:** next to `DarkSoulsIII.exe`
     (`steamapps\common\DARK SOULS III\Game`).
3. Launch the game normally.

> `winmm.dll` is a proxy: it forwards every real Windows `winmm` call on to the
> system `winmm.dll` and runs the fix on startup. Both games import `winmm`, so
> the same file works for either.

### Steam Deck / Linux (Proton)
Add to the game's launch options so Proton loads the proxy DLL:

```
WINEDLLOVERRIDES="winmm=n,b" %command%
```

## Configuration
Edit **`FromFix.ini`** (in the game folder), then restart the game. Only the
section for the game you're running is used.

### Sekiro
| Section | Key | Default | Description |
|---|---|---|---|
| `Borderless Windowed` | `Enabled` | `false` | Run windowed mode as borderless. |
| `Gameplay FOV` | `Multiplier` | `1.00` | FOV scale (`0.01`–`4.00`). |
| `Unlock Framerate` | `Enabled` | `true` | Remove the 60 FPS cap. |
| `Unlock Resolutions` | `Enabled` | `true` | Unlock the windowed resolution list. |
| `Fix Aspect Ratio` | `Enabled` | `true` | Stop forced 16:9 scaling. |
| `Fix HUD` | `Enabled` | `true` | Fix vignettes/fades at non-16:9. |

### Dark Souls III
| Section | Key | Default | Description |
|---|---|---|---|
| `DS3 Skip Intro` | `Enabled` | `true` | Skip the startup logo movies. |
| `DS3 Unlock Framerate` | `Enabled` | `false` | Unlock via the debug limiter. |
| `DS3 Unlock Framerate` | `TargetFPS` | `60` | Desired FPS cap (`30`–`1000`). |
| `DS3 Borderless` | `Enabled` | `false` | Borderless at desktop resolution. |
| `DS3 Resolution` | `Width` / `Height` | `0` | Native/internal resolution (ultrawide, Steam Deck). `0` = off. |

DS3 renders at a hardcoded internal resolution reference (1280x720). Setting
`[DS3 Resolution] Width`/`Height` makes FromFix swap that reference in memory at
startup so the game renders natively at your resolution / aspect — e.g.
`Width = 3440`, `Height = 1440`.

## Verifying it works
On launch, FromFix writes **`FromFix.log`** next to the game exe. It reports the
detected game and, for each fix, the resolved address (or a `pattern scan
failed` line if a signature didn't match — e.g. after a game update).

## Uninstalling
Delete `winmm.dll`, `FromFix.ini` and `FromFix.log` from the game folder. All
patches are applied in memory at runtime, so removing the DLL fully reverts them.

## Building from source
Prerequisites: [Rust](https://rustup.rs/) with the MSVC toolchain and Visual
Studio Build Tools (MSVC linker + Windows SDK).

```sh
cargo build --release
```

Output: `target/release/winmm.dll`. The CRT is linked statically, so there's no
VC++ redist dependency.

### How it's built
- Exports come from `winmm.def`; `build.rs` turns each into an absolute-path
  forwarder to the real system winmm (and links `winmm.lib` so the linker can
  validate them), which keeps the proxy from importing itself.

### Layout
- `src/` — the plugin: `memory` (scan/patch/wait), `state` (config), `config`
  (ini parser), `sekiro` (Sekiro fixes), `ds3` (Dark Souls III fixes), `logger`,
  `lib` (`DllMain` + game detection).
- `vendor/ilhook/` — a vendored fork of [ilhook](https://github.com/regomne/ilhook-rs)
  patched to expose `xmm0`–`xmm15` in its `Registers`.

## Credits
- [ilhook](https://github.com/regomne/ilhook-rs) — the inline-hook engine
  (vendored + patched to expose all xmm registers).
- Dark Souls III FPS unlock: **0dm / saucy** —
  [DS3DebugFPS](https://github.com/0dm/DS3DebugFPS).
- Dark Souls III No-Intro: **bladecoding** —
  [DarkSouls3RemoveIntroScreens](https://github.com/bladecoding/DarkSouls3RemoveIntroScreens).
- Dark Souls III FOV patch: **Altimor**.

## Troubleshooting
- **Game won't start / instantly closes:** ensure `winmm.dll` sits next to the
  game exe; remove it to confirm the game runs vanilla.
- **No effect in-game:** check `FromFix.log`. `pattern scan failed` / `not found`
  lines mean a signature needs updating for your game version.
- **DS3 physics feel wrong:** lower `TargetFPS` back to `60`.
