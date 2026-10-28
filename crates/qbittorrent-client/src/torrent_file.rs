//! Just enough bencode to show what a `.torrent` file contains before adding it.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<Value<'a>>),
    Dict(BTreeMap<&'a [u8], Value<'a>>),
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    depth: usize,
}

/// Untrusted input: bound the nesting so a crafted file can't overflow the stack.
const MAX_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn value(&mut self) -> Option<Value<'a>> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return None;
        }
        let value = match *self.data.get(self.pos)? {
            b'i' => {
                self.pos += 1;
                let end = self.pos + self.data[self.pos..].iter().position(|&b| b == b'e')?;
                let number = std::str::from_utf8(&self.data[self.pos..end])
                    .ok()?
                    .parse()
                    .ok()?;
                self.pos = end + 1;
                Value::Int(number)
            }
            b'l' => {
                self.pos += 1;
                let mut list = Vec::new();
                while *self.data.get(self.pos)? != b'e' {
                    list.push(self.value()?);
                }
                self.pos += 1;
                Value::List(list)
            }
            b'd' => {
                self.pos += 1;
                let mut dict = BTreeMap::new();
                while *self.data.get(self.pos)? != b'e' {
                    let Value::Bytes(key) = self.value()? else {
                        return None;
                    };
                    let value = self.value()?;
                    dict.insert(key, value);
                }
                self.pos += 1;
                Value::Dict(dict)
            }
            b'0'..=b'9' => {
                let colon = self.pos + self.data[self.pos..].iter().position(|&b| b == b':')?;
                let len: usize = std::str::from_utf8(&self.data[self.pos..colon])
                    .ok()?
                    .parse()
                    .ok()?;
                let start = colon + 1;
                let end = start.checked_add(len)?;
                let bytes = self.data.get(start..end)?;
                self.pos = end;
                Value::Bytes(bytes)
            }
            _ => return None,
        };
        self.depth -= 1;
        Some(value)
    }
}

/// Summary of a `.torrent` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentFileInfo {
    pub name: String,
    /// Total size of all files, in bytes
    pub total_size: i64,
    pub file_count: usize,
}

impl TorrentFileInfo {
    /// Reads the `info` dictionary; returns `None` if the data isn't a torrent.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let mut parser = Parser {
            data,
            pos: 0,
            depth: 0,
        };
        let Value::Dict(root) = parser.value()? else {
            return None;
        };
        let Value::Dict(info) = root.get(b"info".as_slice())? else {
            return None;
        };

        let text = |key: &[u8]| match info.get(key) {
            Some(Value::Bytes(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        };
        let name = text(b"name.utf-8").or_else(|| text(b"name"))?;

        let (total_size, file_count) = match (
            info.get(b"length".as_slice()),
            info.get(b"files".as_slice()),
        ) {
            (Some(Value::Int(length)), _) => (*length, 1),
            (_, Some(Value::List(files))) => {
                let lengths = files.iter().map(|file| match file {
                    Value::Dict(file) => match file.get(b"length".as_slice()) {
                        Some(Value::Int(length)) => *length,
                        _ => 0,
                    },
                    _ => 0,
                });
                (lengths.sum(), files.len())
            }
            // BitTorrent v2-only torrents use a file tree
            (_, _) if info.contains_key(b"file tree".as_slice()) => (0, 0),
            _ => return None,
        };

        Some(Self {
            name,
            total_size,
            file_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file() {
        let data = b"d8:announce3:url4:infod6:lengthi1048576e4:name8:file.iso12:piece lengthi16384e6:pieces0:ee";
        let info = TorrentFileInfo::parse(data).unwrap();
        assert_eq!(info.name, "file.iso");
        assert_eq!(info.total_size, 1_048_576);
        assert_eq!(info.file_count, 1);
    }

    #[test]
    fn multiple_files_and_utf8_name() {
        let data = "d4:infod5:filesld6:lengthi100e4:pathl5:a.txteed6:lengthi50e4:pathl5:b.txteee4:name5:album10:name.utf-86:Álbume\
                    e";
        let info = TorrentFileInfo::parse(data.as_bytes()).unwrap();
        assert_eq!(info.name, "Álbum");
        assert_eq!(info.total_size, 150);
        assert_eq!(info.file_count, 2);
    }

    #[test]
    fn rejects_garbage() {
        assert!(TorrentFileInfo::parse(b"").is_none());
        assert!(TorrentFileInfo::parse(b"<html></html>").is_none());
        assert!(
            TorrentFileInfo::parse(b"d4:infod4:name1:xee").is_none(),
            "no length or files"
        );
        assert!(
            TorrentFileInfo::parse(b"d4:infod4:name99:xee").is_none(),
            "truncated"
        );
        assert!(
            TorrentFileInfo::parse(&[b'l'; 10_000]).is_none(),
            "deeply nested"
        );
    }
}
