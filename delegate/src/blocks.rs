//! Byte-preserving managed markdown blocks. Marker lines are deliberately strict.

use std::collections::BTreeSet;
use std::ops::Range;

use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub(super) struct Block {
    pub name: String,
    pub range: Range<usize>,
}

fn marker(line: &[u8]) -> Option<(bool, String)> {
    let (begin, name) = if let Some(rest) = line.strip_prefix(b"<!-- delegate:begin ") {
        (true, rest)
    } else {
        (false, line.strip_prefix(b"<!-- delegate:end ")?)
    };
    let name = name.strip_suffix(b" -->")?;
    if name.is_empty()
        || !name
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return None;
    }
    Some((begin, String::from_utf8_lossy(name).into_owned()))
}

pub(super) fn parse(file: &str, bytes: &[u8]) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut names = BTreeSet::new();
    let mut open: Option<(String, usize, usize)> = None;
    let mut offset = 0;
    for (index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let number = index + 1;
        if let Some((begin, name)) = marker(line.strip_suffix(b"\n").unwrap_or(line)) {
            if begin {
                if let Some((outer, _, _)) = &open {
                    bail!(
                        "{file}:{number}: nested block {name} inside {outer}; fix the markers and retry"
                    );
                }
                if !names.insert(name.clone()) {
                    bail!(
                        "{file}:{number}: duplicate block {name}; give each block a unique name and retry"
                    );
                }
                open = Some((name, offset, number));
            } else {
                let Some((outer, start, _)) = open.take() else {
                    bail!(
                        "{file}:{number}: end {name} without its begin; fix the markers and retry"
                    );
                };
                if outer != name {
                    bail!(
                        "{file}:{number}: end {name} does not match begin {outer}; fix the markers and retry"
                    );
                }
                blocks.push(Block {
                    name,
                    range: start..offset + line.len(),
                });
            }
        }
        offset += line.len();
    }
    if let Some((name, _, number)) = open {
        bail!("{file}:{number}: begin {name} without its end; close the block and retry");
    }
    Ok(blocks)
}

pub(super) fn outside(file: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut start = 0;
    for block in parse(file, bytes)? {
        result.extend_from_slice(&bytes[start..block.range.start]);
        start = block.range.end;
    }
    result.extend_from_slice(&bytes[start..]);
    Ok(result)
}

pub(super) fn append(bytes: &mut Vec<u8>, block: &[u8]) {
    if !bytes.is_empty() {
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.push(b'\n');
    }
    bytes.extend_from_slice(block);
}

/// Match a historical section as an unchanged subsequence of whole lines.
/// Only lines between the first and last matches move with the section.
pub(super) fn adopt(project: &[u8], section: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let lines: Vec<_> = project.split_inclusive(|b| *b == b'\n').collect();
    let expected: Vec<_> = section.split_inclusive(|b| *b == b'\n').collect();
    let mut matches = Vec::new();
    let mut cursor = 0;
    for line in expected {
        let found = lines[cursor..]
            .iter()
            .position(|candidate| *candidate == line)?
            + cursor;
        matches.push(found);
        cursor = found + 1;
    }
    let first = *matches.first()?;
    let last = *matches.last()?;
    let mut kept = Vec::new();
    let mut moved = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if index < first || index > last {
            kept.extend_from_slice(line);
        } else if matches.binary_search(&index).is_err() {
            moved.extend_from_slice(line);
        }
    }
    Some((kept, moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_exact_whole_lines() {
        for text in [
            "prefix <!-- delegate:begin lead -->\n",
            " <!-- delegate:begin lead -->\n",
            "<!-- delegate:begin lead --> suffix\n",
            "<!-- delegate:begin lead -->\r\n",
            "<!-- delegate:begin INVALID -->\n",
        ] {
            assert!(parse("AGENTS.md", text.as_bytes()).unwrap().is_empty());
        }
        let bytes = b"ours\n<!-- delegate:begin lead -->\nkit\n<!-- delegate:end lead -->\ntail";
        let parsed = parse("AGENTS.md", bytes).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "lead");
        assert_eq!(outside("AGENTS.md", bytes).unwrap(), b"ours\ntail");
        let error = parse("AGENTS.md", b"ours\n<!-- delegate:begin lead -->\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("AGENTS.md:2"));
        assert!(error.contains("without its end"));
    }

    #[test]
    fn adoption_keeps_prefix_suffix_and_moves_only_inserted_lines() {
        let project = b"prefix\nheading\nkit one\nour rule\nkit two\nsuffix\n";
        let (kept, moved) = adopt(project, b"heading\nkit one\nkit two\n").unwrap();
        assert_eq!(kept, b"prefix\nsuffix\n");
        assert_eq!(moved, b"our rule\n");
        assert!(adopt(project, b"heading\nkit changed\nkit two\n").is_none());
    }
}
