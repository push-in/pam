#![no_main]

//! Random operation sequences on a small shared table must always agree with
//! a HashMap model, and every table-wide invariant must hold after each step.

use std::collections::HashMap;

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../src/shared_table.rs"]
mod shared_table;

use shared_table::{Layout, Patch, Write};

const VALUE: usize = 24;

fuzz_target!(|data: &[u8]| {
    let capacity = 1 + (data.first().copied().unwrap_or(0) % 32) as u64;
    let layout = Layout::new(capacity, 8, VALUE as u32, 1).unwrap();
    let table = shared_table::open("", layout).unwrap();
    let rows = u64::from(layout.capacity);
    let mut model: HashMap<Vec<u8>, [u8; VALUE]> = HashMap::new();

    for chunk in data.get(1..).unwrap_or_default().chunks(4) {
        let [op, key, a, b] = <[u8; 4]>::try_from(chunk).unwrap_or([chunk[0], 0, 0, 0]);
        // Keys up to 9 bytes: some exceed the 8-byte key size on purpose.
        let key = vec![b'k'; (key % 10) as usize];
        let too_long = key.len() > 8;
        match op % 6 {
            0 | 1 => {
                let offset = (a as usize) % VALUE;
                let length = ((b as usize) % (VALUE + 4)).min(VALUE + 4 - offset);
                let bytes = vec![b; length];
                let result = table.set(&key, &[Patch { offset: offset as u32, bytes: &bytes }]);
                if too_long || offset + length > VALUE {
                    assert!(result.is_err());
                    continue;
                }
                match result.unwrap() {
                    Write::Stored => {
                        let row = model.entry(key).or_insert([0; VALUE]);
                        row[offset..offset + length].copy_from_slice(&bytes);
                    }
                    Write::Full => assert!(!model.contains_key(&key) && model.len() as u64 == rows),
                }
            }
            2 => assert_eq!(table.delete(&key), model.remove(&key).is_some()),
            3 => {
                let delta = i64::from(a) - 128;
                let offset = 8 * (u32::from(b) % 3);
                match table.increment_int(&key, offset, delta) {
                    Err(_) => assert!(too_long),
                    Ok(None) => assert!(!model.contains_key(&key) && model.len() as u64 == rows),
                    Ok(Some(value)) => {
                        let row = model.entry(key).or_insert([0; VALUE]);
                        let at = offset as usize;
                        let next = i64::from_ne_bytes(row[at..at + 8].try_into().unwrap()).wrapping_add(delta);
                        row[at..at + 8].copy_from_slice(&next.to_ne_bytes());
                        assert_eq!(value, next);
                    }
                }
            }
            4 => {
                let mut out = [0_u8; VALUE];
                let found = table.get(&key, &mut out);
                assert_eq!(found, model.contains_key(&key));
                if found {
                    assert_eq!(&out, model.get(&key).unwrap());
                }
                assert_eq!(table.exists(&key), found);
            }
            _ => table.collect(),
        }
        assert_eq!(table.count(), model.len() as u64);
    }

    let mut cursor = 0;
    let mut buffer = Vec::new();
    let scanned = table.scan(&mut cursor, usize::MAX, &mut buffer);
    assert_eq!(scanned, model.len());
});
