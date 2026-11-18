//! The cryptography Red Alert uses to protect MIX file headers.
//!
//! An encrypted header starts with a Blowfish key that is itself encrypted with Westwood's
//! RSA-style public key (`PKStraw`, `PK.CPP`); the header that follows is Blowfish
//! encrypted in ECB mode (`BlowStraw`, `BLOWFISH.CPP`).
//!
//! None of this is real security: the "public" key is used for decryption and is embedded
//! in the game, and the moduli are only 320 bits. It exists so the files can be read.

mod bigint;
mod blowfish;
mod blowfish_tables;
mod pk;

pub use blowfish::Blowfish;
pub use pk::{PublicKey, WESTWOOD_KEY};
