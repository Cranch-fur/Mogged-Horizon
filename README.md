# Mogged for Horizon

<p><b>English</b> · <a href="README_RU.md">Русский</a></p>

A port of [Mogged](../Mogged/README.md) to [Horizon](https://github.com/horizon-rust/horizon) (Minecraft Java 26.3 in Rust) as
a native mod. It switches between your account skin and two presets with one key, with the Fabric mod's head-to-feet
sweep. Other players who have the mod see your presets.

It shares the Fabric mod's version (1.1.0) but does less for now; see below.

## What works

- **Two presets** besides your account skin (*Default*), classic or slim model.
- **K** (rebindable in Options → Mods → Mogged) goes to the next preset in `cycle` that has a skin, with the glowing
  sweep and a sound.
- **Sources:** a PNG file (absolute, or relative to `config/`), a `file:` URI, or an http(s) URL. With none set,
  `config/mogged/preset1.png` and `preset2.png` are used.
- **HD skins** (128×128 to 1024×1024) load but are **scaled down to 64×64**: Horizon keeps every skin in a 64×64 atlas
  slot. Old 64×32 skins are converted.
- **Multiplayer:** single player, LAN, and Horizon servers built with this mod. The server (Horizon's integrated one in single player and on
  LAN) relays presets to the other players with the mod and catches up those who join later.
- **The Fabric mod's config,** `config/mogged.json`: presets, `activePreset`, `cycle`, `durationMs` and `effects` are
  read and saved; keys this port doesn't use (capes, elytra) stay as they are.
- **Settings page** (Options → Mods → Mogged → Settings): which skin to wear (the account's, or a preset that has a
  skin), each such preset's model, animation length, effects, where each preset's skin is read from, and Open skins
  folder.
- **Skins load by themselves:** about once a second the mod checks `mogged.json` and the presets' files (by their
  contents' hash), and reads again whatever appeared, changed or went. A preset given as a URL is downloaded again when
  the URL changes.

## Not yet

- The skin editor, file pickers, drag and drop: paths go in `config/mogged.json`, or files in `config/mogged/`.
- Capes and elytra.
- HD resolution (scaled to 64×64, above).
- Particles; the switch sound plays only for your own switch.
- Network compatibility with the Fabric mod: Horizon mods have their own message channel, so Fabric Mogged players
  and players with this mod don't see each other's presets.
- Vanilla, Paper or Fabric servers: they carry no Horizon mod messages, so only you see your presets.
- Flashback.

## Install

1. A Horizon launcher instance on **Nightly 2026-11-01** (commit `c9d474d`): the mod is pinned to it, see below.
2. Build the `.hnmod` (`python3 tools/package.py` → `target/mogged-1.1.0.hnmod`) or use a released one.
3. Put it in `<instance>/horizon-native-mods/` (**Open folder** on any native mod's row of the launcher's Mods page)
   and press **Build** on the "Native build of …" row. The first build downloads Rust and takes 20 to 30 minutes.

   From the command line: `horizon-launcher native-build <folder with the unpacked .hnmod> --trust-native-code`.
4. Start the game. The mod writes `config/mogged.json` and makes `config/mogged/`; put `preset1.png` and `preset2.png`
   there (or paths in `mogged.json`); they load within a second.

Horizon's native mods aren't sandboxed: they are Rust compiled into the game and run with your rights.

## Why it is pinned

Nearly everything uses Horizon's stable native hook points (`native_hooks.rs`): tick, frame, key, mod messages,
players joining and leaving, the Options → Mods page. There is no hook point for the skin a player is drawn with,
though, so the mod hooks the internal `Game::entity_geometry` and `Game::draw_inventory` (`HEAD`) in `game.rs` and
`Game::face_skin` (`RETURN`) in `game/remote.rs`, turns the settings page's skin paths into text
(`OptionsUi::native_rows`, `RETURN`, in `horizon-client-render/src/options_ui.rs`), adds a module to
`game/remote.rs`, and one to
`horizon-client-render/src/skins.rs` to read the account skin's pixels for the sweep. Those aren't part of the stable
API, so `compatibility.json` names an exact commit rather than a hooks version. For a newer Nightly, check that
`update_remote_skins`, `entity_geometry`, `draw_inventory`, `face_skin`, `native_rows`, `store_skin` and `skins::Downloader` are unchanged and move the pin.

## Build and test

```sh
cargo test                      # the logic (runtime.rs): config, sweep, protocol, relay
python3 tools/package.py        # target/mogged-1.1.0.hnmod
```

On Windows `build.bat` does the same (double-click it), on Linux and macOS `./build.sh`. Python 3.11+ is needed.
This only packs the sources: Horizon's launcher compiles the mod when you press **Build**.

In-game checks go through Horizon's tooling (`horizon-rust/horizon-mods`, `native/tools/native.py check`) or a launcher
build.

| File | What it is |
|---|---|
| `mixins/mogged/horizon-mixin.toml` | The bundle: three modules and the hooks |
| `mixins/mogged/runtime.rs` | Logic and state with no game types (`game::mogged`); also this crate's library, for its tests |
| `mixins/mogged/glue.rs` | The edges with the game (`game::mogged_glue`): files, the skin atlas, messages, sound |
| `mixins/mogged/pixels.rs` | The account skin's pixels and an HTTP GET (`horizon_client::skins::mogged_access`) |
| `mixins/mogged/*.rs` | One small helper per hook |

## Protocol

Horizon mod messages (`native_net_*`), channels `mogged:*`. Client to server: the payload; server to clients: the
sender's UUID (16 bytes, big-endian), then the payload.

- `mogged:skin`: preset (1 or 2), slim (0/1), then 64×64 RGBA (16,384 bytes) or nothing (cleared).
- `mogged:state`: active preset (0–2), animate (0/1), duration in ms (u16 LE), effects (0/1).
- `mogged:remove` (server only): that player left.

## License

GPL-3.0, as Mogged.
