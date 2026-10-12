//! A deliberately read-only reader for active Put Back records.
//!
//! Format reference: https://metacpan.org/dist/Mac-Finder-DSStore/view/DSStoreFormat.pod
//! Offsets in the buddy allocator exclude the initial four-byte version word.

use std::collections::{BTreeMap, HashSet};

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PutBack {
    pub name: Option<String>,
    pub location: Option<String>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or("length overflow")?;
        let bytes = self.bytes.get(self.pos..end).ok_or("truncated block")?;
        self.pos = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String> {
        let len = (self.u32()? as usize)
            .checked_mul(2)
            .ok_or("string too long")?;
        let bytes = self.take(len)?;
        char::decode_utf16(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_be_bytes([b[0], b[1]])),
        )
        .collect::<std::result::Result<String, _>>()
        .map_err(|_| "invalid UTF-16")
    }

    fn value(&mut self, retain: bool) -> Result<Option<String>> {
        let tag = self.take(4)?;
        let len = match tag {
            b"ustr" if retain => return self.string().map(Some),
            b"ustr" => (self.u32()? as usize)
                .checked_mul(2)
                .ok_or("string too long")?,
            b"blob" => self.u32()? as usize,
            b"bool" => 1,
            b"long" | b"shor" | b"type" => 4,
            b"comp" | b"dutc" => 8,
            _ => return Err("unknown value type"),
        };
        self.take(len)?;
        if retain {
            Err("Put Back value is not a string")
        } else {
            Ok(None)
        }
    }
}

fn slice(data: &[u8], offset: u32, len: usize) -> Result<&[u8]> {
    let start = (offset as usize).checked_add(4).ok_or("offset overflow")?;
    let end = start.checked_add(len).ok_or("length overflow")?;
    data.get(start..end).ok_or("block out of bounds")
}

fn block<'a>(data: &'a [u8], addresses: &[u32], id: u32) -> Result<&'a [u8]> {
    let address = *addresses.get(id as usize).ok_or("invalid block id")?;
    let len = 1usize
        .checked_shl(address & 31)
        .ok_or("invalid block size")?;
    slice(data, address & !31, len)
}

pub fn parse(data: &[u8]) -> Result<BTreeMap<String, PutBack>> {
    if data.len() > MAX_BYTES {
        return Err("store too large");
    }
    let mut header = Cursor::new(data);
    if header.u32()? != 1 || header.take(4)? != b"Bud1" {
        return Err("invalid header");
    }
    let offset = header.u32()?;
    let size = header.u32()? as usize;
    if header.u32()? != offset {
        return Err("inconsistent root offsets");
    }
    header.take(16)?;
    let mut root = Cursor::new(slice(data, offset, size)?);
    let count = root.u32()? as usize;
    root.take(4)?;
    let padded = count.checked_add(255).ok_or("address count overflow")? / 256 * 256;
    let table = root.take(padded.checked_mul(4).ok_or("address table overflow")?)?;
    let addresses: Vec<u32> = table
        .as_chunks::<4>()
        .0
        .iter()
        .take(count)
        .map(|b| u32::from_be_bytes(*b))
        .collect();
    let toc_count = root.u32()?;
    let mut dsdb = None;
    for _ in 0..toc_count {
        let len = root.take(1)?[0] as usize;
        let name = root.take(len)?;
        let id = root.u32()?;
        if name == b"DSDB" && dsdb.replace(id).is_some() {
            return Err("duplicate DSDB");
        }
    }
    let mut master = Cursor::new(block(data, &addresses, dsdb.ok_or("missing DSDB")?)?);
    let root_id = master.u32()?;
    master.take(16)?;
    let mut stack = vec![root_id];
    let mut visited = HashSet::new();
    let mut result: BTreeMap<String, PutBack> = BTreeMap::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            return Err("repeated tree node");
        }
        let mut node = Cursor::new(block(data, &addresses, id)?);
        let last_child = node.u32()?;
        let records = node.u32()?;
        for _ in 0..records {
            if last_child != 0 {
                stack.push(node.u32()?);
                if stack.len() > addresses.len() {
                    return Err("too many child pointers");
                }
            }
            let name = node.string()?;
            let key = node.take(4)?;
            let wanted = key == b"ptbN" || key == b"ptbL";
            let value = node.value(wanted)?;
            if wanted {
                let entry = result.entry(name).or_default();
                let slot = if key == b"ptbN" {
                    &mut entry.name
                } else {
                    &mut entry.location
                };
                if slot.is_some() {
                    return Err("duplicate Put Back field");
                }
                *slot = value;
            }
        }
        if last_child != 0 {
            stack.push(last_child);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/put_back.ds_store");

    #[test]
    fn reads_active_records_and_ignores_unallocated_bytes() {
        let mut bytes = FIXTURE.to_vec();
        let expected = parse(&bytes).unwrap();
        assert_eq!(
            expected["renamed.txt"],
            PutBack {
                name: Some("original.txt".into()),
                location: Some("Users/example/work/".into())
            }
        );
        assert_eq!(expected["雪.txt"].name.as_deref(), Some("雪.txt"));
        bytes.extend_from_slice(b"ptbNustrstale-nameptbLustrstale-location");
        assert_eq!(parse(&bytes).unwrap(), expected);
    }

    #[test]
    fn truncated_and_corrupt_inputs_do_not_panic() {
        for len in 0..FIXTURE.len() {
            let _ = parse(&FIXTURE[..len]);
        }
        for index in 0..FIXTURE.len() {
            let mut bytes = FIXTURE.to_vec();
            bytes[index] ^= 0xff;
            let _ = parse(&bytes);
        }
        assert!(parse(b"not a store").is_err());
        let mut bad = FIXTURE.to_vec();
        bad[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse(&bad).is_err());
    }

    fn record(name: &str, key: &[u8; 4], value: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for (index, string) in [name, value].into_iter().enumerate() {
            if index == 1 {
                bytes.extend_from_slice(key);
                bytes.extend_from_slice(b"ustr");
            }
            let units: Vec<u16> = string.encode_utf16().collect();
            bytes.extend_from_slice(&(units.len() as u32).to_be_bytes());
            for unit in units {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
        }
        bytes
    }

    #[test]
    fn reads_internal_records_and_rejects_cycles_and_unknown_types() {
        // Extend the independent fixture's allocator with two leaf pages. The root
        // contributes ptbN itself, and its left child contributes the matching ptbL.
        let mut bytes = FIXTURE.to_vec();
        bytes[2052..2056].copy_from_slice(&5u32.to_be_bytes());
        bytes[2072..2076].copy_from_slice(&(4096u32 | 10).to_be_bytes());
        bytes[2076..2080].copy_from_slice(&(5120u32 | 10).to_be_bytes());
        let mut root = Vec::new();
        for word in [4u32, 1, 3] {
            root.extend_from_slice(&word.to_be_bytes());
        }
        root.extend(record("renamed.txt", b"ptbN", "original.txt"));
        bytes[8196..8196 + root.len()].copy_from_slice(&root);
        let mut left = vec![0; 4];
        left.extend_from_slice(&1u32.to_be_bytes());
        left.extend(record("renamed.txt", b"ptbL", "Users/example/work/"));
        bytes[4100..4100 + left.len()].copy_from_slice(&left);
        bytes[5124..5132].fill(0);
        assert_eq!(
            parse(&bytes).unwrap()["renamed.txt"].name.as_deref(),
            Some("original.txt")
        );
        let mut cycle = bytes.clone();
        cycle[8196..8200].copy_from_slice(&2u32.to_be_bytes());
        assert!(parse(&cycle).is_err());
        let value_tag = 8196 + 12 + 4 + "renamed.txt".len() * 2 + 4;
        bytes[value_tag..value_tag + 4].copy_from_slice(b"????");
        assert!(parse(&bytes).is_err());
        assert!(parse(&vec![0; MAX_BYTES + 1]).is_err());
    }
}
