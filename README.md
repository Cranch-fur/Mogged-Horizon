<p align="center">
  <b>English</b> · <a href="README_RU.md">Русский</a>
</p>

# Mogged for Horizon

A port of [Mogged](https://github.com/Cranch-fur/Mogged) to [Horizon](https://github.com/horizon-rust/horizon) (Minecraft
Java 26.3 in Rust) as a native mod. It lets you switch between your own account skin and two custom presets with a single
key, with a smooth animated transition, and other players who have the mod see your look too.

## Differences from the Fabric mod

- No skin editor, file pickers or drag and drop: put `preset1.png` and `preset2.png` in `config/mogged/`, or set paths
  and URLs in `config/mogged.json`. Changes are picked up within a second.
- HD skins are scaled down to 64×64: Horizon keeps every skin in a 64×64 slot.
- No capes, elytra, particles or Flashback support.
- Fabric and Horizon players don't see each other's presets, and on vanilla, Paper or Fabric servers only you see yours.

Settings (Options → Mods → Mogged): which skin to wear, each preset's model, animation length and effects. The key
(<kbd>K</kbd>) is rebound there too.

## Requirements

| Minecraft | Horizon | Platform |
|---|---|---|
| 26.3 | Nightly 2026-11-01 (`c9d474d`) | PC |

The mod hooks into parts of Horizon that aren't a stable API, so it is pinned to that Nightly. Native mods aren't
sandboxed: they are compiled into the game and run with your rights.

## Install

1. Put `mogged-1.1.0.hnmod` in `<instance>/horizon-native-mods/` (**Open folder** on the launcher's Mods page), or add
   this repository by its GitHub link.
2. Press **Build** on the "Native build" row. The first build downloads Rust and takes 20–30 minutes.
3. Start the game and add your presets to `config/mogged/`.

## Building

Python 3.11+ is needed. On Windows run `build.bat`, on Linux and macOS `./build.sh`. The file ends up in
`target/mogged-1.1.0.hnmod`; this only packs the sources, the launcher compiles them. `cargo test` runs the mod's
logic tests.

## Project layout

```
mixins/mogged/horizon-mixin.toml   the bundle: modules and hooks
mixins/mogged/runtime.rs           the mod's logic (config, animation, protocol), with its tests
mixins/mogged/glue.rs              the link with the game: files, skins, messages, sound
mixins/mogged/*.rs                 one small helper per hook
tools/package.py                   packs the .hnmod
```

Settings are stored in `config/mogged.json`, in the Fabric mod's format.

## Authors

@cranch_fur and @murkatel.

## License

Copyright (C) 2026 @cranch_fur, @murkatel

Mogged is free software under the [GNU General Public License v3.0](LICENSE).
