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
- Standard library only, except `cnc-sim`, which is built on Bevy's ECS (`bevy_ecs`, no
  default features). The front end will add more dependencies.
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
| `cnc-td` | Tiberian Dawn: theaters, houses, missions, generated object types and built-in stats, `.BIN` template maps, scenario objects, triggers and team types |
| `cnc-ra` | Red Alert: theaters, houses, generated object types (incl. vessels), `[MapPack]` / `[OverlayPack]`, typed `RULES.INI` / `AFTRMATH.INI`, scenario objects, `[Trigs]` and `[TeamTypes]` |
| `cnc-assets` | Palettes, SHP keyframe shapes, terrain icon sets (both games' layouts), AUD audio (SOS ADPCM and Westwood codecs) with WAV export |
| `cnc-sim` | The simulation on `bevy_ecs`: shared object components, the tick schedule (events → objects → logic → sync), houses, vehicle movement, the TD map and its scenario loading, the original per-frame sync CRC plus sectioned state digests and sync history, single- or multithreaded execution with identical results |
| `cnc-editor` | Editor model: `CellMetrics`, `CellGrid`, `Scenario` load/save for both games (map layers, objects, waypoints, cell triggers, triggers, team types) |

## Status

Done (with tests):

- [x] Westwood CRC (`CRC.CPP`, `Calculate_CRC`), checked against a transliteration of the assembly
- [x] LCW decode (`LCWUNCMP.CPP`) and encode (editor `WWCompression.cs`, with hash chains instead of the quadratic search)
- [x] MIX archives, classic and RA extended headers (`MIXFILE.CPP`), including RA's
      encrypted headers (`BLOWFISH.CPP`, `PK.CPP`, `INT.CPP`/`MP.CPP`)
- [x] MEG archives (editor `Megafile.cs`)
- [x] INI parsing/writing with RA's rules and typed getters (`INI.CPP`)
- [x] `[MapPack]` / `[OverlayPack]` packing (`LCWStraw`/`LCWPipe` + Base64)
- [x] `fixed` 16.16 (`FIXED.H/CPP`)
- [x] `Sim_Random` (TD) and `RandomClass` (RA)
- [x] `COORDINATE`/`CELL` helpers, `Distance`, `Desired_Facing8/256`, adjacency tables
- [x] Object type enums for both games, cross-checked against the editor's own tables
- [x] Scenario map layers for both games, round-tripping through text
- [x] RA rules: the 171 global settings generated from `RULES.H`/`RULES.CPP` by
      `tools/gen_ra_rules.py` (with the header comments as docs), plus land types,
      power-ups, missions, house biases and per-object stats for units, infantry,
      ships, aircraft, buildings, weapons, warheads and projectiles
- [x] Scenario objects for both games (units, ships, aircraft, infantry, structures,
      terrain, smudges, TD overlays, waypoints, cell triggers) with the original line
      formats and `strtok`/`atoi` parsing, plus triggers and team types (TD's name-based
      format; RA's numeric `NewINIFormat` 2+ format)
- [x] MSVC `rand()` (`CrtRand`), which TD folds into its sync CRC and uses for scattering
- [x] Simulation skeleton (`cnc-sim`): object components, heap order, the tick schedule,
      `Compute_Game_CRC`/`Add_CRC`, and tests that single- and multithreaded runs give
      the same CRC every frame and that unordered conflicting systems are rejected
- [x] Palettes (`.PAL`), SHP shapes (`KEYFRAME.CPP`, `XORDELTA.ASM`), terrain icon sets
      (`ICONSET.CPP`), AUD audio (`ADPCM.CPP` tables; the type-1 Westwood codec from its
      public description, since its decoder isn't in the release)
- [x] Determinism plumbing: `cnc_core::heap::ObjectHeap` (the original heaps' lowest-free
      slot choice and creation-order iteration, with generation-checked ids) and
      `cnc_core::state_hash` (platform-independent FNV-1a state hashing, with
      `impl_state_hash!` for structs)
- [x] Desync diagnosis: `cnc_core::sync` (`StateDigest`, a full-state hash per named
      section that lists which sections two peers disagree on; `SyncHistory`, recent
      frames' sync values with peer checks and first-divergence search) and
      `cnc_core::deferred` (effects queued by parallel workers, applied in heap order).
      In `cnc-sim`: every core component and resource hashed per section, games register
      their own with `hash_component`/`hash_resource`, CRC history with `check_peer`,
      optional digest recording every N frames, and tests that a parallel system using
      deferred effects matches a serial run and that a digest names desyncs the
      original CRC can't see
- [x] TD map (`cnc_sim::map`, `cnc_sim::scenario`): the `Map` resource with per-cell
      template, overlay, overlay data, ground type, ground overrides and occupants;
      `Recalc_Attributes`, overlay placement (`OverlayClass::Mark`: wall, Tiberium, crate
      rules), `Wall_Update`, `Tiberium_Adjust`, `Overpass` (with its skipped bottom row and
      `IRandom` re-rolls), the Remastered per-mission land fixes, and scenario loading of
      the `.BIN` (with the missing-icon check against theater art), terrain objects and
      overlays in file order. Tested on all 47 campaign maps. Generated data now also
      covers templates, terrain objects, overlays and `Ground[]` speeds.
      Not yet: roads and concrete placement (no shipped map uses them), units and
      buildings from the scenario, and the original's intrusive `Next` occupier links (a
      multi-cell object can only be in one cell's chain; the port keeps a list per cell)
- [x] TD vehicle movement (`cnc_sim::drive`), slice 1: the generated track tables
      (`Track1`..`Track13`, `RawTracks`, `TrackControl`), `FacingClass`
      (`cnc_core::geometry::Turning`), `While_Moving` with mid-track jumps between
      curves, `Start_Of_Move`, `Do_Turn` (three-point turns behind an option),
      `Smooth_Turn`, `UnitClass::Can_Enter_Cell`, occupancy with the original's
      per-kind cell flags (`Occupy_Down`/`Occupy_Up`), `Mark_Track` reservations,
      `Stop_Driver`/`Start_Driver`, arrival, `Assign_Destination`; a houses resource
      (alliances, human player, ground speed bias); vehicles loaded from scenarios
      (all 668 in the campaign). Runs in a new `Phase::Objects`, one vehicle at a time in
      logic order. Tested against hand-worked timings (a straight cell takes 31 ticks for
      a medium tank) and single- vs multithreaded digests every frame.
      Not yet: path finding (`Basic_Path`), crushing, crates, scattering, attacking
      blockers, cloaking, radio contact, triggers, threat maps, missions
- [x] TD built-in type data: every unit, infantry, building, aircraft and projectile
      constructor in the `*DATA.CPP` files plus `CONST.CPP`'s weapon and warhead tables,
      generated by `tools/gen_td_data.py` with a real `#ifdef`/`#else` pass (so `PATCH`
      values apply, `ADVANCED`/`OBSOLETE` don't), ordered as the game's `Pointers[]`
      tables index them. Not yet: infantry `do_table`s, RA's built-in defaults

Next, roughly in dependency order:

1. **Remaining assets**: VQA video (its codec isn't in the release), WSA animations,
   fonts, Remastered TGA/ZIP art via MEG.
2. **Built-in type data**: Red Alert's `*DATA.CPP` constructor defaults (which
   `RULES.INI` overrides), TD infantry animation tables, and TD anim/overlay/terrain data.
3. **Rest of scenario loading**: briefings, RA's pre-2 trigger/team formats, and names
   for trigger events, actions and team missions. (`[Basic]`, house sections and
   `[Base]` have typed read-only views in `cnc_core::scenario`.)
4. **Simulation core**: `MAP`/`CELL`, `OBJECT` → `TECHNO` → `FOOT` → unit classes,
   `FINDPATH.CPP`, `COMBAT.CPP`, `BULLET`, `ANIM`, `HOUSE`, `FACTORY`, `LOGIC.CPP`.
   This is the bulk of the work. The class hierarchy and global heaps (`HEAP.CPP`) become
   Bevy ECS entities and components in `cnc-sim`. Determinism rules (see the crate docs):
   the tick schedule treats any unordered pair of conflicting systems as a build error;
   order-dependent systems sort by `HeapIndex` (the original heap order); no parallel
   iteration writes shared state; integer maths only. Renderers (2D now, a 3D mod later)
   are separate Bevy plugins that only read the simulation's components.
5. **Lockstep networking and events**: `EVENT.CPP`, `QUEUE.CPP` (the IPX/modem layers are
   obsolete and won't be ported as-is).
6. **Front end**: rendering, input, sidebar, audio. This needs dependencies (windowing,
   GPU, audio) and is where the std-only rule ends.
7. **Editor**: the rest of `Model/` (objects, triggers, teams), tools, then a UI.

Not planned: DOS/Win95 platform code (`DPMI`, `IPX*`, `MONOC`, DirectDraw), and the
inline assembly kept in `MiscAsm.cpp` beyond the routines the simulation needs.

## Testing against the real game

`crates/cnc-editor/tests/real_data.rs` checks the readers against an installed copy of the
Remastered Collection: every MIX it can find (TD plain, RA encrypted and nested), the MEG
archives, all 47 TD campaign maps in `GENERAL.MIX` (load, save, reload), loose TD and RA
community maps, and RA's `RULES.INI`. It is skipped unless `CNC_REMASTERED_DIR` is set:

```sh
CNC_REMASTERED_DIR="C:/Program Files (x86)/Steam/steamapps/common/CnCRemastered" \
    cargo test -p cnc-editor --test real_data -- --nocapture
```

`crates/cnc-assets/tests/real_data.rs` does the same for art and sound: every TD object
shape, every template in the three TD theaters, all TD sounds, and RA's palettes, object
shapes, templates in all three theaters and sounds from the nested archives. The scenario
tests parse every object, trigger and team type in the TD and RA campaign maps and check
that they write back unchanged.

Findings from the real data so far: the game writes clear `.BIN` cells as `255, 0` (the
editor wrote `255, 255`), `RULES.INI` doesn't define the Aftermath units or the ants, some SOS-compressed sounds
declare one more sample in their final chunk than they contain, some Remastered
`.SHP` files are 0x0 stubs (e.g. RA `SIDEBAR.SHP`), and RA template colour maps have one
entry per template cell, not per icon.

## Building

```sh
cd rust
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## License

Like the C++ it is ported from, this code is GPL-3.0 with the additional terms in
`LICENSE.md` at the repository root.
