use std::collections::HashMap;

pub trait Internable {
    type Storage: Storage<Self>;
}

pub trait Storage<T: Internable + ?Sized> {
    fn new() -> Self;
    fn push(&mut self, value: &T) -> (u32, u32);
    fn get(&self, offset: u32, len: u32) -> &T;
}

pub struct Interner<T: Internable + ?Sized> {
    atoms: Vec<InnerAtom>,
    lookup: HashMap<String, usize>, // Maps string content to atom index
    counts: Vec<usize>,             // Frequency count for each atom
    storage: T::Storage,
    optimized: bool,
    serialized_offsets: Vec<u32>, // Offsets after serialization (set by serialize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atom {
    handle: usize,
}

#[derive(Debug, Clone)]
struct InnerAtom {
    offset: u32,
    len: u32,
}

impl Internable for str {
    type Storage = StringStorage;
}

impl Internable for [u8] {
    type Storage = ArrayStorage;
}

pub struct StringStorage {
    buf: String,
}

pub struct ArrayStorage {
    buf: Vec<u8>,
}

impl Storage<str> for StringStorage {
    fn new() -> Self {
        StringStorage { buf: String::new() }
    }

    fn push(&mut self, value: &str) -> (u32, u32) {
        let offset = self.buf.len() as u32;
        let len = value.len() as u32;
        self.buf.push_str(value);
        (offset, len)
    }

    fn get(&self, offset: u32, len: u32) -> &str {
        &self.buf[offset as usize..(offset + len) as usize]
    }
}

impl Storage<[u8]> for ArrayStorage {
    fn new() -> Self {
        ArrayStorage { buf: Vec::new() }
    }

    fn push(&mut self, value: &[u8]) -> (u32, u32) {
        let offset = self.buf.len() as u32;
        let len = value.len() as u32;
        self.buf.extend_from_slice(value);
        (offset, len)
    }

    fn get(&self, offset: u32, len: u32) -> &[u8] {
        &self.buf[offset as usize..(offset + len) as usize]
    }
}

impl<T: Internable + ?Sized> Default for Interner<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Internable + ?Sized> Interner<T> {
    pub fn new() -> Self {
        Interner {
            atoms: Vec::new(),
            lookup: HashMap::new(),
            counts: Vec::new(),
            storage: T::Storage::new(),
            optimized: false,
            serialized_offsets: Vec::new(),
        }
    }
}

impl Interner<str> {
    pub fn intern(&mut self, s: &str) -> Atom {
        if let Some(&index) = self.lookup.get(s) {
            self.counts[index] += 1;
            return Atom { handle: index };
        }

        let (offset, len) = self.storage.push(s);
        let index = self.atoms.len();

        self.atoms.push(InnerAtom { offset, len });
        self.counts.push(1);
        self.lookup.insert(s.to_string(), index);

        Atom { handle: index }
    }

    pub fn atoms(&self) -> impl Iterator<Item = (Atom, usize)> + '_ {
        self.atoms
            .iter()
            .enumerate()
            .map(|(idx, _)| (Atom { handle: idx }, self.counts[idx]))
    }

    pub fn resolve(&self, atom: Atom) -> Option<&str> {
        if atom.handle < self.atoms.len() {
            let inner = &self.atoms[atom.handle];
            Some(self.storage.get(inner.offset, inner.len))
        } else {
            None
        }
    }

    pub fn optimize(&mut self) {
        if self.atoms.is_empty() {
            return;
        }

        // Create a vector of (index, count, length) for sorting
        let mut items: Vec<(usize, usize, u32)> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(idx, atom)| (idx, self.counts[idx], atom.len))
            .collect();

        // Sort by frequency (descending), then by length (ascending)
        items.sort_by(|a, b| {
            b.1.cmp(&a.1) // Higher frequency first
                .then_with(|| a.2.cmp(&b.2)) // Shorter strings first if same frequency
        });

        // Create new storage with reordered strings
        let mut new_storage = StringStorage::new();
        let mut new_atoms = Vec::with_capacity(self.atoms.len());
        let mut old_to_new = vec![0usize; self.atoms.len()];
        let mut new_counts = Vec::with_capacity(self.counts.len());

        for (new_idx, &(old_idx, count, _)) in items.iter().enumerate() {
            let old_atom = &self.atoms[old_idx];
            let s = self.storage.get(old_atom.offset, old_atom.len);
            let (offset, len) = new_storage.push(s);

            new_atoms.push(InnerAtom { offset, len });
            new_counts.push(count);
            old_to_new[old_idx] = new_idx;
        }

        // Update lookup table with new indices
        for (_, index) in self.lookup.iter_mut() {
            *index = old_to_new[*index];
        }

        self.storage = new_storage;
        self.atoms = new_atoms;
        self.counts = new_counts;
        self.optimized = true;
    }

    // TODO: Serialize into an io::Write?
    #[cfg(feature = "serialize")]
    pub fn serialize(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::new();
        self.serialized_offsets.clear();

        let mut current_offset = 0u32;

        for atom in &self.atoms {
            self.serialized_offsets.push(current_offset);

            let s = self.storage.get(atom.offset, atom.len);
            let len = s.len();

            // Write ULEB128 encoded length
            let len_bytes = leb128_usize(len as u64)?;
            leb128::write::unsigned(&mut result, len as u64)?;
            // result.extend_from_slice(&len_bytes);

            // Write string bytes
            result.extend_from_slice(s.as_bytes());

            current_offset += (len_bytes + len) as u32;
        }

        Ok(result)
    }
}

impl Interner<[u8]> {
    pub fn intern(&mut self, bytes: &[u8]) -> Atom {
        // Use bytes as a key for lookup
        let key = bytes.to_vec();
        let key_str = format!("{:?}", key); // Simple way to create a hashable key

        if let Some(&index) = self.lookup.get(&key_str) {
            self.counts[index] += 1;
            return Atom { handle: index };
        }

        let (offset, len) = self.storage.push(bytes);
        let index = self.atoms.len();

        self.atoms.push(InnerAtom { offset, len });
        self.counts.push(1);
        self.lookup.insert(key_str, index);

        Atom { handle: index }
    }

    pub fn optimize(&mut self) {
        if self.atoms.is_empty() {
            return;
        }

        // Create a vector of (index, count, length) for sorting
        let mut items: Vec<(usize, usize, u32)> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(idx, atom)| (idx, self.counts[idx], atom.len))
            .collect();

        // Sort by frequency (descending), then by length (ascending)
        items.sort_by(|a, b| {
            b.1.cmp(&a.1) // Higher frequency first
                .then_with(|| a.2.cmp(&b.2)) // Shorter arrays first if same frequency
        });

        // Create new storage with reordered byte arrays
        let mut new_storage = ArrayStorage::new();
        let mut new_atoms = Vec::with_capacity(self.atoms.len());
        let mut old_to_new = vec![0usize; self.atoms.len()];
        let mut new_counts = Vec::with_capacity(self.counts.len());

        for (new_idx, &(old_idx, count, _)) in items.iter().enumerate() {
            let old_atom = &self.atoms[old_idx];
            let bytes = self.storage.get(old_atom.offset, old_atom.len);
            let (offset, len) = new_storage.push(bytes);

            new_atoms.push(InnerAtom { offset, len });
            new_counts.push(count);
            old_to_new[old_idx] = new_idx;
        }

        // Update lookup table with new indices
        for (_, index) in self.lookup.iter_mut() {
            *index = old_to_new[*index];
        }

        self.storage = new_storage;
        self.atoms = new_atoms;
        self.counts = new_counts;
        self.optimized = true;
    }

    #[cfg(feature = "serialize")]
    pub fn serialize(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut result = Vec::new();
        self.serialized_offsets.clear();

        let mut current_offset = 0u32;

        for atom in &self.atoms {
            self.serialized_offsets.push(current_offset);

            let bytes = self.storage.get(atom.offset, atom.len);
            let len = bytes.len();

            // Write ULEB128 encoded length
            let len_bytes = leb128_usize(len as u64)?;
            leb128::write::unsigned(&mut result, len as u64)?;

            // Write byte array
            result.extend_from_slice(bytes);

            current_offset += (len_bytes + len) as u32;
        }

        Ok(result)
    }
}

impl Atom {
    pub fn resolve_index(&self, interner: &Interner<str>) -> Option<u32> {
        if self.handle < interner.serialized_offsets.len() {
            Some(interner.serialized_offsets[self.handle])
        } else {
            None
        }
    }

    pub fn resolve_index_bytes(&self, interner: &Interner<[u8]>) -> Option<u32> {
        if self.handle < interner.serialized_offsets.len() {
            Some(interner.serialized_offsets[self.handle])
        } else {
            None
        }
    }
}

// Calculate written size of an unsigned leb128 representation.
#[cfg(feature = "serialize")]
fn leb128_usize(val: u64) -> Result<usize, std::io::Error> {
    let mut c = std::io::Cursor::new([0u8; 10]);
    leb128::write::unsigned(&mut c, val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_interning() {
        let mut interner = Interner::<str>::new();

        let atom1 = interner.intern("hello");
        let atom2 = interner.intern("world");
        let atom3 = interner.intern("hello");

        assert_eq!(atom1, atom3);
        assert_ne!(atom1, atom2);
    }

    #[test]
    fn test_optimize() {
        let mut interner = Interner::<str>::new();

        let a1 = interner.intern("rare");
        let a2 = interner.intern("common");
        interner.intern("common");
        interner.intern("common");
        let a5 = interner.intern("short");
        interner.intern("short");
        interner.intern("short");

        // Before optimization, check original handles
        assert_eq!(a1.handle, 0); // "rare" is first
        assert_eq!(a2.handle, 1); // "common" is second
        assert_eq!(a5.handle, 2); // "short" is third

        interner.optimize();

        // After optimization, the Atom handles don't change for existing atoms,
        // but the lookup table is updated so new interns get the reordered indices
        let new_short = interner.intern("short");
        let new_common = interner.intern("common");
        let new_rare = interner.intern("rare");

        // After optimization:
        // Index 0: "short" (3 uses, length 5)
        // Index 1: "common" (3 uses, length 6)
        // Index 2: "rare" (1 use, length 4)
        assert_eq!(new_short.handle, 0); // "short" should be at index 0
        assert_eq!(new_common.handle, 1); // "common" should be at index 1
        assert_eq!(new_rare.handle, 2); // "rare" should be at index 2
    }

    #[cfg(feature = "serialize")]
    #[test]
    fn test_serialize() {
        let mut interner = Interner::<str>::new();

        let a1 = interner.intern("hello");
        let a2 = interner.intern("world");

        let serialized = interner.serialize().unwrap();

        // Check that we can resolve indices
        assert_eq!(a1.resolve_index(&interner), Some(0));
        assert!(a2.resolve_index(&interner).is_some());

        // Verify serialized data is not empty
        assert!(!serialized.is_empty());
    }

    #[cfg(feature = "serialize")]
    #[test]
    fn test_optimize_and_serialize() {
        let mut interner = Interner::<str>::new();

        let a1 = interner.intern("abc");
        interner.intern("abc");
        interner.intern("abc");
        let a4 = interner.intern("xy");
        interner.intern("xy");
        let a6 = interner.intern("z");

        interner.optimize();

        // After optimize: "abc" (3 uses), "xy" (2 uses), "z" (1 use)
        assert_eq!(a1.handle, 0);
        assert_eq!(a4.handle, 1);
        assert_eq!(a6.handle, 2);

        let serialized = interner.serialize().unwrap();

        println!("{:?}", serialized);

        // All atoms should have valid offsets
        assert_eq!(a1.resolve_index(&interner), Some(0));
        assert!(a4.resolve_index(&interner).unwrap() > 0);
        assert!(a6.resolve_index(&interner).unwrap() > a4.resolve_index(&interner).unwrap());
        assert!(!serialized.is_empty());
    }

    #[test]
    fn test_basic_byte_interning() {
        let mut interner = Interner::<[u8]>::new();

        let atom1 = interner.intern(&[1, 2, 3]);
        let atom2 = interner.intern(&[4, 5, 6]);
        let atom3 = interner.intern(&[1, 2, 3]);

        assert_eq!(atom1, atom3);
        assert_ne!(atom1, atom2);
    }

    #[test]
    fn test_byte_optimize() {
        let mut interner = Interner::<[u8]>::new();

        let a1 = interner.intern(&[0xFF, 0xEE]);
        let a2 = interner.intern(&[0x01, 0x02, 0x03]);
        interner.intern(&[0x01, 0x02, 0x03]);
        interner.intern(&[0x01, 0x02, 0x03]);
        let a5 = interner.intern(&[0xAA, 0xBB]);
        interner.intern(&[0xAA, 0xBB]);
        interner.intern(&[0xAA, 0xBB]);

        // Before optimization, check original handles
        assert_eq!(a1.handle, 0); // [0xFF, 0xEE] is first
        assert_eq!(a2.handle, 1); // [0x01, 0x02, 0x03] is second
        assert_eq!(a5.handle, 2); // [0xAA, 0xBB] is third

        interner.optimize();

        // After optimization, verify reordering through new interns
        let new_aa = interner.intern(&[0xAA, 0xBB]);
        let new_01 = interner.intern(&[0x01, 0x02, 0x03]);
        let new_ff = interner.intern(&[0xFF, 0xEE]);

        // After optimization:
        // Index 0: [0xAA, 0xBB] (3 uses, length 2)
        // Index 1: [0x01, 0x02, 0x03] (3 uses, length 3)
        // Index 2: [0xFF, 0xEE] (1 use, length 2)
        assert_eq!(new_aa.handle, 0);
        assert_eq!(new_01.handle, 1);
        assert_eq!(new_ff.handle, 2);
    }

    #[test]
    #[cfg(feature = "serialize")]
    fn test_byte_serialize() {
        let mut interner = Interner::<[u8]>::new();

        let a1 = interner.intern(&[0x01, 0x02]);
        let a2 = interner.intern(&[0xAA, 0xBB, 0xCC]);

        let serialized = interner.serialize().unwrap();

        // Check that we can resolve indices
        assert_eq!(a1.resolve_index_bytes(&interner), Some(0));
        assert!(a2.resolve_index_bytes(&interner).is_some());

        // Verify serialized data is not empty
        assert!(!serialized.is_empty());
    }

    #[test]
    #[cfg(feature = "serialize")]
    fn test_byte_optimize_and_serialize() {
        let mut interner = Interner::<[u8]>::new();

        let a1 = interner.intern(&[0x01]);
        interner.intern(&[0x01]);
        interner.intern(&[0x01]);
        let a4 = interner.intern(&[0x02, 0x03]);
        interner.intern(&[0x02, 0x03]);
        let a6 = interner.intern(&[0x04, 0x05, 0x06]);

        interner.optimize();

        // After optimize: [0x01] (3 uses), [0x02, 0x03] (2 uses), [0x04, 0x05, 0x06] (1 use)
        assert_eq!(a1.handle, 0);
        assert_eq!(a4.handle, 1);
        assert_eq!(a6.handle, 2);

        let serialized = interner.serialize().unwrap();

        // All atoms should have valid offsets
        assert_eq!(a1.resolve_index_bytes(&interner), Some(0));
        assert!(a4.resolve_index_bytes(&interner).unwrap() > 0);
        assert!(
            a6.resolve_index_bytes(&interner).unwrap() > a4.resolve_index_bytes(&interner).unwrap()
        );
        assert!(!serialized.is_empty());
    }

    #[test]
    fn test_byte_array_different_lengths() {
        let mut interner = Interner::<[u8]>::new();

        // Test with arrays of different lengths
        let short = interner.intern(&[1]);
        let medium = interner.intern(&[2, 3, 4]);
        let long = interner.intern(&[5, 6, 7, 8, 9]);
        let empty = interner.intern(&[]);

        assert_ne!(short, medium);
        assert_ne!(medium, long);
        assert_ne!(short, empty);

        // Verify empty array works
        let empty2 = interner.intern(&[]);
        assert_eq!(empty, empty2);
    }

    #[test]
    fn test_byte_frequency_sorting() {
        let mut interner = Interner::<[u8]>::new();

        // Create atoms with different frequencies but same length
        interner.intern(&[1, 1]);
        interner.intern(&[2, 2]);
        interner.intern(&[2, 2]);
        interner.intern(&[2, 2]);
        interner.intern(&[2, 2]);
        interner.intern(&[3, 3]);
        interner.intern(&[3, 3]);
        interner.intern(&[3, 3]);

        interner.optimize();

        // Most frequent should be first
        let new_common = interner.intern(&[2, 2]);
        let new_mid = interner.intern(&[3, 3]);
        let new_rare = interner.intern(&[1, 1]);

        assert_eq!(new_common.handle, 0); // 4 uses
        assert_eq!(new_mid.handle, 1); // 3 uses
        assert_eq!(new_rare.handle, 2); // 1 use
    }
}
