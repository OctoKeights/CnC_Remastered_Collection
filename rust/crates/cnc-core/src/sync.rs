//! Desync detection and diagnosis.
//!
//! The original sync check (`Compute_Game_CRC`) is one 32-bit number per frame covering
//! object positions and facings. It says *when* peers diverged, not *what* diverged. This
//! module adds the tools to answer the second question:
//!
//! * [`StateDigest`]: a full-state hash split into named sections (one per component or
//!   resource), so comparing two digests names the parts of the state that differ.
//! * [`SyncHistory`]: the last N frames' sync values, so a report from a peer about an
//!   old frame can still be checked, and two histories can be searched for the first frame
//!   they disagree on.

use std::collections::VecDeque;

use crate::state_hash::{StateHash, StateHasher};

/// A hash of the whole simulation state, kept per section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDigest {
    sections: Vec<(&'static str, u64)>,
}

impl StateDigest {
    /// Starts an empty digest.
    #[must_use]
    pub const fn builder() -> DigestBuilder {
        DigestBuilder {
            sections: Vec::new(),
        }
    }

    /// One hash over every section, names included.
    #[must_use]
    pub fn total(&self) -> u64 {
        let mut hasher = StateHasher::new();
        for &(name, hash) in &self.sections {
            hasher.write(name.as_bytes());
            hasher.add(&hash);
        }
        hasher.finish()
    }

    /// The sections, in the order they were added.
    #[must_use]
    pub fn sections(&self) -> &[(&'static str, u64)] {
        &self.sections
    }

    /// The hash of one section.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<u64> {
        self.sections
            .iter()
            .find(|&&(n, _)| n == name)
            .map(|&(_, hash)| hash)
    }

    /// Names of sections whose hashes differ from `other`'s, or that only one side has.
    #[must_use]
    pub fn differences(&self, other: &Self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self
            .sections
            .iter()
            .filter(|&&(name, hash)| other.section(name) != Some(hash))
            .map(|&(name, _)| name)
            .collect();
        out.extend(
            other
                .sections
                .iter()
                .filter(|&&(name, _)| self.section(name).is_none())
                .map(|&(name, _)| name),
        );
        out
    }
}

/// Builds a [`StateDigest`] one section at a time.
#[derive(Debug, Clone)]
pub struct DigestBuilder {
    sections: Vec<(&'static str, u64)>,
}

impl DigestBuilder {
    /// Adds a section whose content `fill` writes into a fresh hasher.
    pub fn section(
        &mut self,
        name: &'static str,
        fill: impl FnOnce(&mut StateHasher),
    ) -> &mut Self {
        let mut hasher = StateHasher::new();
        fill(&mut hasher);
        self.sections.push((name, hasher.finish()));
        self
    }

    /// Adds a section holding one value.
    pub fn value<T: StateHash + ?Sized>(&mut self, name: &'static str, value: &T) -> &mut Self {
        self.section(name, |hasher| {
            hasher.add(value);
        })
    }

    /// Finishes the digest.
    #[must_use]
    pub fn build(&mut self) -> StateDigest {
        StateDigest {
            sections: std::mem::take(&mut self.sections),
        }
    }
}

/// What one frame's sync value was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRecord {
    /// The frame number.
    pub frame: u32,
    /// The value peers compare (the original CRC, or a digest total).
    pub value: u64,
}

/// The outcome of checking a peer's sync value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncCheck {
    /// Both sides computed the same value.
    InSync,
    /// The values differ: the games have diverged by this frame.
    Desync {
        /// Our value.
        local: u64,
        /// Theirs.
        remote: u64,
    },
    /// The frame isn't in the history (not reached yet, or too old).
    Unknown,
}

/// The sync values of the most recent frames.
#[derive(Debug, Clone)]
pub struct SyncHistory {
    records: VecDeque<SyncRecord>,
    capacity: usize,
}

impl SyncHistory {
    /// Keeps the last `capacity` frames (at least one).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            records: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Records a frame's value, dropping the oldest if full. A frame already present (from
    /// a re-simulation) is replaced along with every later one.
    pub fn record(&mut self, frame: u32, value: u64) {
        if let Some(pos) = self.records.iter().position(|r| r.frame >= frame) {
            self.records.truncate(pos);
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
        }
        self.records.push_back(SyncRecord { frame, value });
    }

    /// The value recorded for `frame`.
    #[must_use]
    pub fn get(&self, frame: u32) -> Option<u64> {
        self.records
            .iter()
            .find(|r| r.frame == frame)
            .map(|r| r.value)
    }

    /// Checks a peer's value for `frame` against ours.
    #[must_use]
    pub fn check(&self, frame: u32, remote: u64) -> SyncCheck {
        match self.get(frame) {
            None => SyncCheck::Unknown,
            Some(local) if local == remote => SyncCheck::InSync,
            Some(local) => SyncCheck::Desync { local, remote },
        }
    }

    /// The earliest frame both histories hold with different values.
    #[must_use]
    pub fn first_divergence(&self, other: &Self) -> Option<u32> {
        self.records
            .iter()
            .find(|r| other.get(r.frame).is_some_and(|v| v != r.value))
            .map(|r| r.frame)
    }

    /// Recorded frames, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = SyncRecord> + '_ {
        self.records.iter().copied()
    }

    /// The most recent record.
    #[must_use]
    pub fn latest(&self) -> Option<SyncRecord> {
        self.records.back().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(units: u32, rng: u8) -> StateDigest {
        StateDigest::builder()
            .value("units", &units)
            .value("rng", &rng)
            .build()
    }

    #[test]
    fn digests_name_what_differs() {
        let a = digest(1, 7);
        assert_eq!(a, digest(1, 7));
        assert_eq!(a.total(), digest(1, 7).total());
        assert_eq!(a.differences(&digest(1, 7)), Vec::<&str>::new());
        assert_eq!(a.differences(&digest(2, 7)), ["units"]);
        assert_ne!(a.total(), digest(2, 7).total());

        let extra = StateDigest::builder()
            .value("units", &1_u32)
            .value("rng", &7_u8)
            .value("bullets", &0_u8)
            .build();
        assert_eq!(a.differences(&extra), ["bullets"]);
    }

    #[test]
    fn history_checks_and_forgets() {
        let mut history = SyncHistory::new(3);
        for frame in 0..5 {
            history.record(frame, u64::from(frame) * 10);
        }
        assert_eq!(history.get(1), None);
        assert_eq!(history.check(4, 40), SyncCheck::InSync);
        assert_eq!(
            history.check(3, 31),
            SyncCheck::Desync {
                local: 30,
                remote: 31
            }
        );
        assert_eq!(history.check(9, 0), SyncCheck::Unknown);
        assert_eq!(
            history.latest(),
            Some(SyncRecord {
                frame: 4,
                value: 40
            })
        );
    }

    #[test]
    fn rerecording_a_frame_drops_later_ones() {
        let mut history = SyncHistory::new(8);
        for frame in 0..4 {
            history.record(frame, 1);
        }
        history.record(2, 9);
        let frames: Vec<u32> = history.iter().map(|r| r.frame).collect();
        assert_eq!(frames, [0, 1, 2]);
        assert_eq!(history.get(2), Some(9));
    }

    #[test]
    fn first_divergence_between_peers() {
        let mut a = SyncHistory::new(16);
        let mut b = SyncHistory::new(16);
        for frame in 0..10 {
            a.record(frame, u64::from(frame));
            b.record(frame, if frame < 6 { u64::from(frame) } else { 0 });
        }
        assert_eq!(a.first_divergence(&b), Some(6));
        assert_eq!(a.first_divergence(&a.clone()), None);
    }
}
