//! Just enough bencode for an nREPL probe client.

use anyhow::{Result, anyhow};
use std::collections::BTreeMap;

pub fn encode_dict(fields: &[(&str, &str)]) -> Vec<u8> {
    let mut sorted = fields.to_vec();
    sorted.sort();
    let mut out = b"d".to_vec();
    for (key, value) in sorted {
        for s in [key, value] {
            out.extend_from_slice(format!("{}:", s.len()).as_bytes());
            out.extend_from_slice(s.as_bytes());
        }
    }
    out.push(b'e');
    out
}

#[derive(Debug)]
enum Item {
    Bytes(String),
    Int,
    List(Vec<Item>),
    Dict(BTreeMap<String, Item>),
}

/// A response dictionary. String fields are kept; `status` keeps its list.
#[derive(Debug, Default)]
pub struct Dict(BTreeMap<String, Item>);

impl Dict {
    pub fn get(&self, key: &str) -> Option<&String> {
        match self.0.get(key) {
            Some(Item::Bytes(s)) => Some(s),
            _ => None,
        }
    }
    pub fn statuses(&self) -> Vec<String> {
        match self.0.get("status") {
            Some(Item::List(items)) => items
                .iter()
                .filter_map(|i| match i {
                    Item::Bytes(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Decodes one dictionary, or returns None when the buffer is incomplete.
pub fn decode_dict(buf: &[u8]) -> Result<Option<(Dict, usize)>> {
    match item(buf, 0)? {
        Some((Item::Dict(map), used)) => Ok(Some((Dict(map), used))),
        Some(_) => Err(anyhow!("expected a bencode dictionary")),
        None => Ok(None),
    }
}

fn item(buf: &[u8], at: usize) -> Result<Option<(Item, usize)>> {
    let Some(&tag) = buf.get(at) else {
        return Ok(None);
    };
    match tag {
        b'i' => Ok(buf[at..]
            .iter()
            .position(|&b| b == b'e')
            .map(|end| (Item::Int, at + end + 1))),
        b'l' | b'd' => {
            let mut pos = at + 1;
            let mut list = Vec::new();
            let mut dict = BTreeMap::new();
            loop {
                match buf.get(pos) {
                    None => return Ok(None),
                    Some(b'e') => {
                        let value = if tag == b'l' {
                            Item::List(list)
                        } else {
                            Item::Dict(dict)
                        };
                        return Ok(Some((value, pos + 1)));
                    }
                    Some(_) => {}
                }
                let Some((first, next)) = item(buf, pos)? else {
                    return Ok(None);
                };
                pos = next;
                if tag == b'l' {
                    list.push(first);
                    continue;
                }
                let Item::Bytes(key) = first else {
                    return Err(anyhow!("non-string dictionary key"));
                };
                let Some((value, next)) = item(buf, pos)? else {
                    return Ok(None);
                };
                pos = next;
                dict.insert(key, value);
            }
        }
        b'0'..=b'9' => {
            let Some(colon) = buf[at..].iter().position(|&b| b == b':') else {
                return Ok(None);
            };
            let len: usize = std::str::from_utf8(&buf[at..at + colon])?.parse()?;
            let start = at + colon + 1;
            if buf.len() < start + len {
                return Ok(None);
            }
            let text = String::from_utf8_lossy(&buf[start..start + len]).into_owned();
            Ok(Some((Item::Bytes(text), start + len)))
        }
        other => Err(anyhow!("unexpected bencode byte {other}")),
    }
}
