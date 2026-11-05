// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounds-checked little-endian reads and the chunk walk shared by the model formats.

pub fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let raw = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(raw.try_into().ok()?))
}

pub fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(raw.try_into().ok()?))
}

pub fn f32_at(bytes: &[u8], at: usize) -> Option<f32> {
    u32_at(bytes, at).map(f32::from_bits)
}

/// The text before the first zero byte (all of it when there is none), and what follows the zero.
pub fn c_string(bytes: &[u8]) -> (String, &[u8]) {
    match bytes.iter().position(|b| *b == 0) {
        Some(end) => (
            String::from_utf8_lossy(bytes.get(..end).unwrap_or_default()).into_owned(),
            bytes.get(end + 1..).unwrap_or_default(),
        ),
        None => (String::from_utf8_lossy(bytes).into_owned(), &[]),
    }
}

/// One chunk: a `u16` id, then a `u32` length that counts the six header bytes too.
pub struct Chunk<'a> {
    pub id: u16,
    /// The chunk's data, cut short when the file ends before its stated length.
    pub body: &'a [u8],
}

/// The chunks that follow one another in `bytes`. A length below six ends the walk: the rest
/// cannot be framed.
pub struct Chunks<'a> {
    rest: &'a [u8],
}

impl<'a> Chunks<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Chunks { rest: bytes }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Chunk<'a>> {
        let id = u16_at(self.rest, 0)?;
        let len = u32_at(self.rest, 2)? as usize;
        if len < 6 {
            self.rest = &[];
            return None;
        }
        let body = self
            .rest
            .get(6..len.min(self.rest.len()))
            .unwrap_or_default();
        self.rest = self.rest.get(len..).unwrap_or_default();
        Some(Chunk { id, body })
    }
}
