//! Little byte-reading helpers shared by the parsers.

use crate::{Error, Result};

pub fn le_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

pub fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

pub fn le_u64(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}

pub fn cstr(bytes: &[u8], off: usize) -> Option<&str> {
    let rest = bytes.get(off..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&rest[..end]).ok()
}

/// A forward-only cursor over a byte slice.
pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    what: &'static str,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], what: &'static str) -> Self {
        Self { data, pos: 0, what }
    }

    pub fn at(data: &'a [u8], pos: usize, what: &'static str) -> Self {
        Self { data, pos, what }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn err(&self) -> Error {
        Error::Malformed(self.what)
    }

    pub fn u8(&mut self) -> Result<u8> {
        let b = *self.data.get(self.pos).ok_or_else(|| self.err())?;
        self.pos += 1;
        Ok(b)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.data.get(self.pos..self.pos + 2).ok_or_else(|| self.err())?;
        self.pos += 2;
        Ok(le_u16(b))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.data.get(self.pos..self.pos + 4).ok_or_else(|| self.err())?;
        self.pos += 4;
        Ok(le_u32(b))
    }

    pub fn u64(&mut self) -> Result<u64> {
        let b = self.data.get(self.pos..self.pos + 8).ok_or_else(|| self.err())?;
        self.pos += 8;
        Ok(le_u64(b))
    }

    pub fn uleb(&mut self) -> Result<u64> {
        let mut result = 0u64;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            if shift < 64 {
                result |= ((b & 0x7f) as u64) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                return Ok(result);
            }
        }
    }

    pub fn sleb(&mut self) -> Result<i64> {
        let mut result = 0i64;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            if shift < 64 {
                result |= ((b & 0x7f) as i64) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                if shift < 64 && b & 0x40 != 0 {
                    result |= -1i64 << shift;
                }
                return Ok(result);
            }
        }
    }

    pub fn cstr(&mut self) -> Result<&'a str> {
        let rest = self.data.get(self.pos..).ok_or_else(|| self.err())?;
        let end = rest.iter().position(|&b| b == 0).ok_or_else(|| self.err())?;
        let s = std::str::from_utf8(&rest[..end]).map_err(|_| self.err())?;
        self.pos += end + 1;
        Ok(s)
    }
}
