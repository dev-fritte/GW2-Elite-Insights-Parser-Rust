//! Compressed sparse row index: for every key a contiguous list of element indices.
//!
//! Built in three passes (count, prefix sum, fill) without hashing or per-key allocations.

/// Marks an element that belongs to no key.
pub const NO_KEY: u32 = u32::MAX;

/// For each key `0..key_count` the indices of the elements that carry this key, in the order
/// the elements were given.
#[derive(Debug, Clone, Default)]
pub struct Csr {
    offsets: Vec<u32>,
    items: Vec<u32>,
}

impl Csr {
    /// Builds the index over `keys`, where `keys[i]` is the key of element `i`, or [`NO_KEY`].
    ///
    /// # Panics
    /// If a key is not below `key_count` or there are more than `u32::MAX` elements.
    #[must_use]
    pub fn build(keys: &[u32], key_count: usize) -> Self {
        assert!(
            u32::try_from(keys.len()).is_ok(),
            "too many elements for a u32 index"
        );
        // pass 1: count
        let mut offsets = vec![0u32; key_count + 1];
        for &k in keys {
            if k != NO_KEY {
                offsets[k as usize + 1] += 1;
            }
        }
        // pass 2: prefix sum
        for i in 0..key_count {
            offsets[i + 1] += offsets[i];
        }
        // pass 3: fill
        let mut cursor = offsets.clone();
        let mut items = vec![0u32; offsets[key_count] as usize];
        for (index, &k) in (0u32..).zip(keys) {
            if k != NO_KEY {
                let slot = &mut cursor[k as usize];
                items[*slot as usize] = index;
                *slot += 1;
            }
        }
        Self { offsets, items }
    }

    /// Replaces every stored element index `i` by `map[i]`. Used to let an index built over a
    /// reordered view of the elements point at the original positions.
    ///
    /// # Panics
    /// If an index is not below `map.len()`.
    pub fn map_items(&mut self, map: &[u32]) {
        for item in &mut self.items {
            *item = map[*item as usize];
        }
    }

    /// Number of keys.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// The element indices of `key`; empty for keys outside the index.
    #[must_use]
    pub fn get(&self, key: u32) -> &[u32] {
        let key = key as usize;
        match (self.offsets.get(key), self.offsets.get(key + 1)) {
            (Some(&start), Some(&end)) => &self.items[start as usize..end as usize],
            _ => &[],
        }
    }

    /// Total number of indexed elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` if no element is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_elements_by_key_in_input_order() {
        let keys = [1, NO_KEY, 0, 1, 2, 0, 1];
        let csr = Csr::build(&keys, 4);
        assert_eq!(csr.key_count(), 4);
        assert_eq!(csr.get(0), [2, 5]);
        assert_eq!(csr.get(1), [0, 3, 6]);
        assert_eq!(csr.get(2), [4]);
        assert_eq!(csr.get(3), [] as [u32; 0]);
        assert_eq!(csr.get(99), [] as [u32; 0]);
        assert_eq!(csr.len(), 6);
    }

    #[test]
    fn empty_input() {
        let csr = Csr::build(&[], 0);
        assert!(csr.is_empty());
        assert_eq!(csr.get(0), [] as [u32; 0]);
    }
}
