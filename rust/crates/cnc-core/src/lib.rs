//! Shared foundations for the Rust port of the Command & Conquer Remastered Collection.
//!
//! Everything in this crate is used by both games (Tiberian Dawn and Red Alert) and by the
//! map editor. It is pure, safe Rust with no dependencies outside the standard library.
//!
//! | Module | Ported from |
//! |---|---|
//! | [`crc`] | `CRC.CPP`, `Calculate_CRC` in `MiscAsm.cpp`, editor `CRC.cs` |
//! | [`crypto`] | `BLOWFISH.CPP`, `PK.CPP`, `INT.CPP`/`MP.CPP` (MIX header encryption) |
//! | [`lcw`] | `LCWUNCMP.CPP`, editor `WWCompression.cs` |
//! | [`base64`] | `BASE64.CPP` |
//! | [`mix`] | `MIXFILE.CPP` (both games) |
//! | [`meg`] | editor `Megafile.cs` |
//! | [`ini`] | `INI.CPP` (RA), `PROFILE.CPP` (TD), editor `INI.cs` |
//! | [`pack`] | `INIComp` / editor `CompressLCWSection` (`[MapPack]`, `[OverlayPack]`) |
//! | [`fixed`] | `FIXED.H` / `FIXED.CPP` |
//! | [`random`] | `RAND.CPP` (TD), `RANDOM.CPP` (RA) |
//! | [`map`] | `[Map]` handling in `DISPLAY.CPP` |
//! | [`named`] | INI-named type enums (`*TypeClass::From_Name`) |
//! | [`scenario`] | object lines in `UNIT.CPP`, `INFANTRY.CPP`, `BUILDING.CPP`, ... `Read_INI`/`Write_INI` |
//! | [`geometry`] | `FUNCTION.H` inlines, `COORD.CPP`, facing routines in `MiscAsm.cpp` |

pub mod base64;
pub mod bytes;
pub mod crc;
pub mod crypto;
pub mod deferred;
pub mod fixed;
pub mod geometry;
pub mod heap;
pub mod ini;
pub mod lcw;
pub mod map;
pub mod meg;
pub mod mix;
pub mod named;
pub mod pack;
pub mod random;
pub mod scenario;
pub mod state_hash;
pub mod sync;

pub use fixed::Fixed;
