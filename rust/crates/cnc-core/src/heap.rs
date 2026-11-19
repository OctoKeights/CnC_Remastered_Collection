//! The original's fixed object heaps (`FixedIHeapClass` in `HEAP.CPP`).
//!
//! Every kind of game object (units, bullets, animations, ...) lives in a fixed-size heap.
//! Two details of it are observable in the simulation, so the port reproduces both:
//!
//! * **Slot choice**: a new object takes the *lowest free* slot (`FreeFlag.First_False()`).
//!   The slot number is the object's identity in save games and network events.
//! * **Iteration order**: the game walks objects through `ActivePointers`, which appends
//!   on allocation and closes the gap on removal, so objects are processed in the order
//!   they were *created*, not in slot order.
//!
//! [`ObjectHeap`] keeps both, plus a generation per slot so a stale [`SlotId`] (one whose
//! object has been freed and its slot reused) is detected instead of silently aliasing.

use std::fmt;

/// A handle to an object in an [`ObjectHeap`]: the slot index plus the generation it was
/// allocated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId {
    index: u16,
    generation: u32,
}

impl SlotId {
    /// The slot number, as the original's `ID()` returns it.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }

    /// How many times this slot had been allocated before this object.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}v{}", self.index, self.generation)
    }
}

/// The heap is full (the original's `Allocate` returns null and the object isn't made).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapFull;

impl fmt::Display for HeapFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("object heap is full")
    }
}

impl std::error::Error for HeapFull {}

/// A fixed-capacity object store with the original's slot and ordering rules.
#[derive(Debug, Clone)]
pub struct ObjectHeap<T> {
    slots: Vec<Option<T>>,
    generations: Vec<u32>,
    /// Occupied slots in allocation order (`ActivePointers`).
    active: Vec<u16>,
}

impl<T> ObjectHeap<T> {
    /// Creates an empty heap with room for `capacity` objects.
    ///
    /// # Panics
    ///
    /// If `capacity` exceeds `u16::MAX + 1` (the original heaps are far smaller).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity <= usize::from(u16::MAX) + 1,
            "heap capacity too large"
        );
        Self {
            slots: (0..capacity).map(|_| None).collect(),
            generations: vec![0; capacity],
            active: Vec::with_capacity(capacity),
        }
    }

    /// The most objects the heap can hold.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// How many objects are live.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Whether no objects are live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Stores `value` in the lowest free slot.
    ///
    /// # Errors
    ///
    /// [`HeapFull`] if every slot is taken; `value` is dropped.
    pub fn insert(&mut self, value: T) -> Result<SlotId, HeapFull> {
        let index = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(HeapFull)?;
        self.slots[index] = Some(value);
        let index = u16::try_from(index).map_err(|_| HeapFull)?;
        self.active.push(index);
        Ok(SlotId {
            index,
            generation: self.generations[usize::from(index)],
        })
    }

    /// Removes and returns the object `id` refers to, if it is still live.
    pub fn remove(&mut self, id: SlotId) -> Option<T> {
        if !self.contains(id) {
            return None;
        }
        let index = usize::from(id.index);
        self.generations[index] = self.generations[index].wrapping_add(1);
        if let Some(pos) = self.active.iter().position(|&slot| slot == id.index) {
            // `Vec::remove` shifts the rest down, as `DynamicVectorClass::Delete` does.
            self.active.remove(pos);
        }
        self.slots[index].take()
    }

    /// Whether `id` refers to a live object.
    #[must_use]
    pub fn contains(&self, id: SlotId) -> bool {
        let index = usize::from(id.index);
        index < self.slots.len()
            && self.generations[index] == id.generation
            && self.slots[index].is_some()
    }

    /// The object `id` refers to, if it is still live.
    #[must_use]
    pub fn get(&self, id: SlotId) -> Option<&T> {
        if self.contains(id) {
            self.slots[usize::from(id.index)].as_ref()
        } else {
            None
        }
    }

    /// The object `id` refers to, mutably, if it is still live.
    pub fn get_mut(&mut self, id: SlotId) -> Option<&mut T> {
        if self.contains(id) {
            self.slots[usize::from(id.index)].as_mut()
        } else {
            None
        }
    }

    /// The live object in slot `index` (as save games and network events name objects).
    #[must_use]
    pub fn id_at(&self, index: u16) -> Option<SlotId> {
        let i = usize::from(index);
        self.slots.get(i)?.as_ref()?;
        Some(SlotId {
            index,
            generation: self.generations[i],
        })
    }

    /// Live objects in allocation order, the order the game processes them in.
    #[must_use = "iterators do nothing unless consumed"]
    pub fn iter(&self) -> impl Iterator<Item = (SlotId, &T)> + '_ {
        self.active.iter().filter_map(move |&index| {
            let i = usize::from(index);
            let id = SlotId {
                index,
                generation: self.generations[i],
            };
            // Active slots are always occupied; `filter_map` just avoids a panic path.
            self.slots[i].as_ref().map(|value| (id, value))
        })
    }

    /// Ids of live objects in allocation order. Collect these first to modify objects (or
    /// the heap) while walking it.
    #[must_use = "iterators do nothing unless consumed"]
    pub fn ids(&self) -> impl ExactSizeIterator<Item = SlotId> + '_ {
        self.active.iter().map(move |&index| SlotId {
            index,
            generation: self.generations[usize::from(index)],
        })
    }

    /// Removes every object (`FixedIHeapClass::Clear`); generations keep counting so old
    /// ids stay invalid.
    pub fn clear(&mut self) {
        for index in self.active.drain(..) {
            let i = usize::from(index);
            self.slots[i] = None;
            self.generations[i] = self.generations[i].wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowest_free_slot_but_creation_order() {
        let mut heap = ObjectHeap::new(4);
        let a = heap.insert('a').expect("free slot");
        let b = heap.insert('b').expect("free slot");
        let c = heap.insert('c').expect("free slot");
        assert_eq!([a.index(), b.index(), c.index()], [0, 1, 2]);

        assert_eq!(heap.remove(a), Some('a'));
        let d = heap.insert('d').expect("free slot");
        // `d` reuses slot 0 but is processed last.
        assert_eq!(d.index(), 0);
        let order: Vec<char> = heap.iter().map(|(_, &v)| v).collect();
        assert_eq!(order, ['b', 'c', 'd']);
    }

    #[test]
    fn stale_ids_are_rejected() {
        let mut heap = ObjectHeap::new(2);
        let a = heap.insert(1).expect("free slot");
        heap.remove(a);
        let b = heap.insert(2).expect("free slot");
        assert_eq!(a.index(), b.index());
        assert_ne!(a, b);
        assert_eq!(heap.get(a), None);
        assert_eq!(heap.remove(a), None);
        assert_eq!(heap.get(b), Some(&2));
        assert_eq!(heap.id_at(0), Some(b));
    }

    #[test]
    fn full_heap() {
        let mut heap = ObjectHeap::new(1);
        heap.insert(()).expect("free slot");
        assert_eq!(heap.insert(()), Err(HeapFull));
        heap.clear();
        assert!(heap.is_empty());
        assert!(heap.insert(()).is_ok());
    }
}
