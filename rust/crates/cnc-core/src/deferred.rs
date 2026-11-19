//! Deferred effects, applied in a fixed order.
//!
//! Parallel systems can't change shared state directly: which worker gets there first
//! would decide the outcome. Instead each worker records what it wants to happen (damage
//! to deal, objects to create, sounds to play) in its own [`Deferred`] buffer, tagged with
//! a key naming the object that caused it. Afterwards the buffers are merged and drained in
//! key order, so the effects land exactly as if the objects had been processed one at a
//! time in that order, however the work was split between threads.
//!
//! The key must identify the producing object (in the sim, its heap order), and all of an
//! object's effects must be recorded by the one worker that processed it. Effects with the
//! same key then keep the order they were recorded in.

/// Effects waiting to be applied, each tagged with the key of the object that caused it.
#[derive(Debug, Clone)]
pub struct Deferred<K, E> {
    effects: Vec<(K, E)>,
}

impl<K, E> Default for Deferred<K, E> {
    fn default() -> Self {
        Self {
            effects: Vec::new(),
        }
    }
}

impl<K: Ord, E> Deferred<K, E> {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an effect caused by the object `key`.
    pub fn push(&mut self, key: K, effect: E) {
        self.effects.push((key, effect));
    }

    /// How many effects are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Whether nothing is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Moves another buffer's effects into this one.
    pub fn append(&mut self, other: &mut Self) {
        self.effects.append(&mut other.effects);
    }

    /// Merges per-worker buffers into one.
    pub fn merge(parts: impl IntoIterator<Item = Self>) -> Self {
        let mut out = Self::new();
        for mut part in parts {
            out.append(&mut part);
        }
        out
    }

    /// Takes every effect in key order (stable, so one object's effects stay in the order
    /// it recorded them).
    pub fn drain_ordered(&mut self) -> impl Iterator<Item = (K, E)> + '_ {
        // A stable sort: equal keys come from the same object, recorded in order.
        self.effects.sort_by(|a, b| a.0.cmp(&b.0));
        self.effects.drain(..)
    }
}

impl<K: Ord, E> Extend<(K, E)> for Deferred<K, E> {
    fn extend<I: IntoIterator<Item = (K, E)>>(&mut self, iter: I) {
        self.effects.extend(iter);
    }
}

impl<K: Ord, E> FromIterator<(K, E)> for Deferred<K, E> {
    fn from_iter<I: IntoIterator<Item = (K, E)>>(iter: I) -> Self {
        Self {
            effects: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Processes objects 0..n split into chunks (as workers would), each object emitting
    /// two effects, and returns the applied order.
    fn run(n: u32, chunk: usize) -> Vec<(u32, char)> {
        let objects: Vec<u32> = (0..n).collect();
        let parts = objects.chunks(chunk).rev().map(|chunk| {
            let mut local = Deferred::new();
            for &object in chunk {
                local.push(object, 'a');
                local.push(object, 'b');
            }
            local
        });
        Deferred::merge(parts).drain_ordered().collect()
    }

    #[test]
    fn worker_split_doesnt_change_the_order() {
        let serial = run(10, 10);
        assert_eq!(&serial[..4], [(0, 'a'), (0, 'b'), (1, 'a'), (1, 'b')]);
        for chunk in 1..=10 {
            assert_eq!(run(10, chunk), serial);
        }
    }

    #[test]
    fn drained_buffers_are_empty() {
        let mut deferred: Deferred<u8, ()> = [(2, ()), (1, ())].into_iter().collect();
        assert_eq!(deferred.len(), 2);
        assert_eq!(
            deferred
                .drain_ordered()
                .map(|(k, ())| k)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(deferred.is_empty());
    }
}
