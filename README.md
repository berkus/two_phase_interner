# Two-phase data interner

A Rust library for interning strings or byte arrays with frequency-based optimization and serialization support.

## Overview

This library provides a two-phase approach to data interning:

1. **Collection phase**: Intern strings or byte arrays and receive `Atom` handles. The interner tracks usage frequency for each unique value.

2. **Optimization phase**: Call `optimize()` to reorder internal storage based on usage patterns. Frequently-used values are placed first, resulting in smaller serialized offsets. The `Atom` handles returned to clients remain unchanged and continue to work correctly after optimization.

The interner supports serialization with ULEB128-encoded length prefixes, allowing efficient storage and retrieval of interned data.

## Features

- Supports interning for both `str` and `[u8]` types
- Frequency tracking for each interned value
- Optimization that reorders storage by:
  - Frequency (higher frequency first)
  - Length (shorter values first when frequency is equal)
- Stable `Atom` handles that remain valid after optimization
- Serialization with ULEB128 length encoding
- Resolution of atoms to byte offsets in serialized data

## Usage

### String Interning

```rust
use two_phase_interner::Interner;

fn main() {
    let mut interner = Interner::<str>::new();

    // Collect phase: intern strings
    let hello = interner.intern("hello");
    let world = interner.intern("world");
    let hello2 = interner.intern("hello"); // Returns same Atom as first "hello"

    assert_eq!(hello, hello2);

    // Resolve atoms back to strings
    assert_eq!(interner.resolve(hello), Some("hello"));
    assert_eq!(interner.resolve(world), Some("world"));

    // Optimization phase: reorder by frequency
    interner.optimize();

    // Atoms still work after optimization
    assert_eq!(interner.resolve(hello), Some("hello"));

    // Serialize with ULEB128 length prefixes
    #[cfg(feature = "serialize")]
    {
        let serialized = interner.serialize().unwrap();

        // Get byte offset of each atom in serialized data
        let offset = hello.resolve_index(&interner).unwrap();
        println!("'hello' starts at byte offset {}", offset);
    }
}
```

### Byte Array Interning

```rust
use two_phase_interner::Interner;

fn main() {
    let mut interner = Interner::<[u8]>::new();

    // Intern byte arrays
    let data1 = interner.intern(&[0x01, 0x02, 0x03]);
    let data2 = interner.intern(&[0xFF, 0xEE]);
    let data3 = interner.intern(&[0x01, 0x02, 0x03]); // Same as data1

    assert_eq!(data1, data3);
    assert_ne!(data1, data2);

    // Optimize storage
    interner.optimize();

    // Serialize and get offsets
    #[cfg(feature = "serialize")]
    {
        let serialized = interner.serialize().unwrap();
        let offset = data1.resolve_index_bytes(&interner).unwrap();
        println!("Byte array starts at offset {}", offset);
    }
}
```

### Iterating Over Atoms

```rust
use two_phase_interner::Interner;

fn main() {
    let mut interner = Interner::<str>::new();
    
    interner.intern("rust");
    interner.intern("rust");
    interner.intern("programming");
    
    // Iterate over all atoms with their usage counts
    for (atom, count) in interner.atoms() {
        let s = interner.resolve(atom).unwrap();
        println!("'{}' was interned {} times", s, count);
    }
}
```

## How Optimization Works

Before optimization, interned values are stored in insertion order. After calling `optimize()`, the internal storage is reordered:

```rust
let mut interner = Interner::<str>::new();

// Intern with different frequencies
interner.intern("rare");           // 1 use
interner.intern("common");         // 3 uses
interner.intern("common");
interner.intern("common");
interner.intern("short");          // 3 uses
interner.intern("short");
interner.intern("short");

interner.optimize();

// After optimization, internal order is:
// 1. "short" (3 uses, 5 bytes) - most frequent, shorter
// 2. "common" (3 uses, 6 bytes) - most frequent, longer
// 3. "rare" (1 use, 4 bytes) - least frequent
```

The `Atom` handles returned before optimization continue to resolve correctly to their original values.

## License

This project is licensed under the MIT License or Apache-2.0 License at your discretion.
