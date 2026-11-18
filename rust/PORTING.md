# Porting the C&C Remastered Collection to Rust

This directory is an idiomatic, safe Rust port of the source in this repository: Tiberian
Dawn (`TIBERIANDAWN/`, ~257k lines of C++), Red Alert (`REDALERT/`, ~360k lines) and the map
editor (`CnCTDRAMapEditor/`, ~61k lines of C#).

The goal is a **standalone engine**, not a drop-in `TiberianDawn.dll` / `RedAlert.dll`
replacement. The Remastered client talks to those DLLs through a raw C ABI
(`DLLInterface.cpp`), which can't be written without `unsafe`. If a drop-in build is wanted
later, it belongs in a separate, small FFI crate that is the only place `unsafe` is
allowed, wrapping these safe crates.

## Rules

- `unsafe_code = "forbid"` across the workspace.
- `cargo fmt --check`, `cargo clippy --all-targets` (pedantic) with `-D warnings`, and
  `cargo test` must pass. CI runs all of them (`.github/workflows/rust.yml`).
- Standard library only for now.
- **Simulation code is bit-exact.** Multiplayer lockstep and replays depend on every peer
  computing the same thing, so RNGs, fixed-point maths, facing and distance routines
  reproduce the original integer arithmetic, quirks included. Each such quirk is
  documented where it lives and pinned by a test (e.g. `desired_facing256` returning 63 for
  due east).
- **File formats are defensive.** Parsers never panic or read out of bounds on bad
  input; they return errors. Where the original silently tolerated bad data, the port
  says so in the docs.
- Identifiers that come from the original data (type enums, INI names, ids) are
  **generated** from the C++ sources by `tools/gen_types.py`, not typed by hand.
  Re-run it and `cargo fmt` after changing the generator.

## Layout

| Crate | Contents |
|---|---|
| `cnc-core` | Shared by everything: CRC, LCW, Base64, MIX, MEG, INI, packed INI sections, `[Map]`, fixed point, RNGs, coordinates/cells/facings, the `named_enum!` macro |
| `cnc-td` | Tiberian Dawn: theaters, houses, generated object types, `.BIN` template maps |
| `cnc-ra` | Red Alert: theaters, houses, generated object types (incl. vessels), `[MapPack]` / `[OverlayPack]` |
| `cnc-editor` | Editor model: `CellMetrics`, `CellGrid`, `Scenario` load/save for both games |

## Status

Done (with tests):

- [x] Westwood CRC (`CRC.CPP`, `Calculate_CRC`), checked against a transliteration of the assembly
- [x] LCW decode (`LCWUNCMP.CPP`) and encode (editor `WWCompression.cs`, with hash chains instead of the quadratic search)
- [x] MIX archives, classic and RA extended headers (`MIXFILE.CPP`)
- [x] MEG archives (editor `Megafile.cs`)
- [x] INI parsing/writing with RA's rules and typed getters (`INI.CPP`)
- [x] `[MapPack]` / `[OverlayPack]` packing (`LCWStraw`/`LCWPipe` + Base64)
- [x] `fixed` 16.16 (`FIXED.H/CPP`)
- [x] `Sim_Random` (TD) and `RandomClass` (RA)
- [x] `COORDINATE`/`CELL` helpers, `Distance`, `Desired_Facing8/256`, adjacency tables
- [x] Object type enums for both games, cross-checked against the editor's own tables
- [x] Scenario map layers for both games, round-tripping through text

Next, roughly in dependency order:

1. **Encrypted RA MIX headers**: `BLOWFISH.CPP`, `PK.CPP`, `INT.CPP` (bignum), `SHA.CPP`.
2. **Assets**: `SHP` shapes (`KEYFRAME.CPP`), palettes, icon sets/templates (`ICONSET`),
   `AUD` audio (`ADPCM.CPP`), VQA video. Remastered TGA/ZIP art via MEG.
3. **Rules and type data**: the `*DATA.CPP` tables as data, `RULES.CPP` / `RULES.INI`
   (RA), `TechnoTypeClass` hierarchy as traits/structs.
4. **Scenario loading**: `SCENARIO.CPP`, `IOMAP.CPP`, `IOOBJ.CPP`, houses, teams,
   triggers (`TRIGGER`, `TEAMTYPE`, RA `TEVENT`/`TACTION`).
5. **Simulation core**: `MAP`/`CELL`, `OBJECT` → `TECHNO` → `FOOT` → unit classes,
   `FINDPATH.CPP`, `COMBAT.CPP`, `BULLET`, `ANIM`, `HOUSE`, `FACTORY`, `LOGIC.CPP`.
   This is the bulk of the work, and replacing the C++ class hierarchy and its global
   heaps (`HEAP.CPP`) with arenas/ids is the main design decision.
6. **Lockstep networking and events**: `EVENT.CPP`, `QUEUE.CPP` (the IPX/modem layers are
   obsolete and won't be ported as-is).
7. **Front end**: rendering, input, sidebar, audio. This needs dependencies (windowing,
   GPU, audio) and is where the std-only rule ends.
8. **Editor**: the rest of `Model/` (objects, triggers, teams), tools, then a UI.

Not planned: DOS/Win95 platform code (`DPMI`, `IPX*`, `MONOC`, DirectDraw), and the
inline assembly kept in `MiscAsm.cpp` beyond the routines the simulation needs.

## Building

```sh
cd rust
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## License

Like the C++ it is ported from, this code is GPL-3.0 with the additional terms in
`LICENSE.md` at the repository root.
