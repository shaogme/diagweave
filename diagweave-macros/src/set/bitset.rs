use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub(crate) fn with_capacity(bits: usize) -> Self {
        let words = bits.div_ceil(64);
        Self {
            words: vec![0; words],
        }
    }

    pub(crate) fn insert(&mut self, bit: usize) -> bool {
        self.ensure(bit);
        let word = bit / 64;
        let mask = 1u64 << (bit % 64);
        let existed = self.words[word] & mask != 0;
        self.words[word] |= mask;
        !existed
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, bit: usize) -> bool {
        let word = bit / 64;
        if word >= self.words.len() {
            return false;
        }
        let mask = 1u64 << (bit % 64);
        self.words[word] & mask != 0
    }

    pub(crate) fn union_with(&mut self, other: &Self) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (i, right) in other.words.iter().enumerate() {
            self.words[i] |= right;
        }
    }

    pub(crate) fn is_subset_of(&self, other: &Self) -> bool {
        for (i, left) in self.words.iter().enumerate() {
            let right = other.words.get(i).copied().unwrap_or(0);
            if left & !right != 0 {
                return false;
            }
        }
        true
    }

    fn ensure(&mut self, bit: usize) {
        let need_words = (bit / 64) + 1;
        if need_words > self.words.len() {
            self.words.resize(need_words, 0);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SymbolTable {
    by_key: BTreeMap<String, usize>,
    len: usize,
}

impl SymbolTable {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn intern(&mut self, key: String) -> usize {
        if let Some(index) = self.by_key.get(&key).copied() {
            return index;
        }
        let index = self.len;
        self.by_key.insert(key, index);
        self.len += 1;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::BitSet;

    fn next(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        *seed
    }

    #[test]
    fn subset_union_basics() {
        let mut a = BitSet::with_capacity(128);
        let mut b = BitSet::with_capacity(128);
        a.insert(1);
        a.insert(65);
        b.insert(1);
        b.insert(2);
        b.insert(65);

        assert!(a.is_subset_of(&b));
        assert!(!b.is_subset_of(&a));

        let mut c = a.clone();
        c.union_with(&b);
        assert!(c.contains(1));
        assert!(c.contains(2));
        assert!(c.contains(65));
    }

    #[test]
    fn repeated_randomized_consistency() {
        for round in 0..200 {
            let mut seed = round as u64 + 1;
            let mut left = BitSet::with_capacity(512);
            let mut right = BitSet::with_capacity(512);
            let mut left_ref = [false; 512];
            let mut right_ref = [false; 512];

            for _ in 0..600 {
                let li = (next(&mut seed) as usize) % 512;
                let ri = (next(&mut seed) as usize) % 512;
                left.insert(li);
                right.insert(ri);
                left_ref[li] = true;
                right_ref[ri] = true;
            }

            let left_subset_right = left_ref
                .iter()
                .zip(right_ref.iter())
                .all(|(l, r)| !*l || *r);
            let right_subset_left = right_ref
                .iter()
                .zip(left_ref.iter())
                .all(|(r, l)| !*r || *l);

            assert_eq!(left.is_subset_of(&right), left_subset_right);
            assert_eq!(right.is_subset_of(&left), right_subset_left);

            let mut union = left.clone();
            union.union_with(&right);
            for (idx, expected) in left_ref
                .iter()
                .zip(right_ref.iter())
                .map(|(l, r)| *l || *r)
                .enumerate()
            {
                assert_eq!(union.contains(idx), expected, "index={idx}, round={round}");
            }
        }
    }
}
