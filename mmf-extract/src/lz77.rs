//! The Clickteam MMF LZ77 decompressor.
//!
//! Transliterated from `FUN_004c557a` in `DMA4.exe` (the MMF2 runtime's
//! shared "uncompress bytes" routine), cross-checked instruction by
//! instruction against `objdump -d DMA4.exe` over `0x4c557a`-`0x4c56fd`.
//!
//! The routine is a byte-oriented LZ77. `eax` walks the input, `ecx` walks
//! the output, `esi` carries the current control/length value and `edi`
//! addresses the back-reference.
//!
//! ## Control byte dispatch (the match table at `0x4c562d`)
//!
//! | control        | meaning                                                   |
//! |----------------|-----------------------------------------------------------|
//! | `0x00..0x0F`   | 2-byte match, distance `(O0 << 2) + (C >> 2) + 1`         |
//! | `0x10..0x1F`   | 3-byte match, distance `(O0 >> 2) + (O1 << 6) + 0x4000+`  |
//! | `0x20..0x3F`   | match, length `C & 0x1F` (0 = extended), 2-byte offset    |
//! | `0x40..0xFF`   | match, length `(C >> 5) + 1`, 1-byte offset                |
//!
//! For the `0x10..0x1F` form the distance gains a further `0x4000` when
//! bit 3 of the control byte is set, giving a 32 KiB window instead of
//! 16 KiB. A zero base offset with bit 3 clear is the end-of-stream
//! marker (`0x4c56bd`).
//!
//! ## Other forms
//!
//! * `0x4c5593` — a control byte above `0x11` means "copy `C - 0x11`
//!   literals first"; the following control byte then selects a match.
//! * `0x4c55ef` — after a literal run, a control byte below `0x10` selects
//!   a 3-byte match with distance `(O0 << 2) + (C >> 2) + 0x801`.
//! * `0x4c5620` — after a match, the low two bits of the byte two back
//!   give the number of trailing literal bytes; the next control byte
//!   then selects a match.
//!
//! Extended lengths are a run of `0x00` bytes (each contributing `0xFF`)
//! followed by a final byte.

use anyhow::{anyhow, bail, Result};

/// Compress `src` into a stream that [`decompress`] reproduces exactly.
///
/// The encoder emits the game's own literal-run forms and the end-of-stream
/// marker, so the result is a valid MMF LZ77 stream. It deliberately does
/// not try to find back-references: its job is to be a faithful, lossless
/// inverse of the decompressor, which is what the round-trip tests check.
///
/// The stream is built as follows:
///
/// * A first byte above `0x11` means "copy `C - 0x11` literals" and so can
///   encode any run of 1 to 238 bytes on its own.
/// * A first byte of `0x00` selects the extended literal run, which can
///   encode an arbitrarily long run.
/// * The stream is closed with the end-of-stream marker `0x11 0x00 0x00`.
pub fn compress(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + src.len() / 64 + 16);

    if src.is_empty() {
        // The end-of-stream marker on its own decodes to nothing.
        out.extend_from_slice(&[0x11, 0x00, 0x00]);
        return out;
    }

    if src.len() <= 238 {
        // `0x4c5593`: copy `C - 0x11` literals.
        out.push(0x11 + src.len() as u8);
        out.extend_from_slice(src);
    } else {
        // `0x4c55b5`: a literal run of `3 + len` bytes, with the length
        // extended by a run of `0x00` bytes each worth `0xFF` plus a final
        // byte. We need `3 + len == src.len()`.
        let target = src.len() - 3; // the `len` the decoder will compute
        // `target == 0xFF * k + 0x0F + b` with `b` in 1..=0xFF.
        let t = target - 0x0F;
        let k = (t - 1) / 0xFF;
        let b = t - 0xFF * k;
        debug_assert!(b >= 1 && b <= 0xFF);
        out.push(0x00); // the literal-run control byte
        out.extend(std::iter::repeat_n(0u8, k));
        out.push(b as u8);
        out.extend_from_slice(src);
    }

    // End-of-stream marker: control `0x11` (a 3-byte offset form with bit 3
    // clear), then a two-byte offset of zero.
    out.extend_from_slice(&[0x11, 0x00, 0x00]);
    out
}

/// Decompress an LZ77 stream.
pub fn decompress(src: &[u8]) -> Result<Vec<u8>> {
    let mut d = Decompressor {
        src,
        pos: 0,
        out: Vec::with_capacity(src.len() * 4),
    };
    d.run()?;
    Ok(d.out)
}

struct Decompressor<'a> {
    src: &'a [u8],
    pos: usize,
    out: Vec<u8>,
}

/// The control-flow states of the original routine.
enum State {
    /// `0x4c55ac`: read a control byte. `>= 0x10` starts a match, below
    /// that it starts a literal run.
    LiteralControl,
    /// `0x4c55e6`: reached after a literal run. `>= 0x10` starts a match,
    /// below that it selects the special 3-byte match at `0x4c55ef`.
    AfterLiteralRun,
    /// `0x4c5616`: after a match. The low two bits of the byte two back
    /// give the number of trailing literal bytes.
    AfterMatch,
}

impl<'a> Decompressor<'a> {
    #[inline]
    fn byte(&mut self) -> Result<u8> {
        let b = *self
            .src
            .get(self.pos)
            .ok_or_else(|| anyhow!("lz77: input exhausted at {}", self.pos))?;
        self.pos += 1;
        Ok(b)
    }

    #[inline]
    fn peek(&self) -> Result<u8> {
        self.src
            .get(self.pos)
            .copied()
            .ok_or_else(|| anyhow!("lz77: input exhausted at {}", self.pos))
    }

    /// Copy `n` literal bytes from the input to the output.
    fn literals(&mut self, n: usize) -> Result<()> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| anyhow!("lz77: length overflow"))?;
        if end > self.src.len() {
            bail!(
                "lz77: literal run of {n} bytes at {} runs past end of input",
                self.pos
            );
        }
        self.out.extend_from_slice(&self.src[self.pos..end]);
        self.pos = end;
        Ok(())
    }

    /// Read an extended length: `0x00` bytes each add `0xFF`, then a final
    /// byte, added to `base`.
    fn extended_len(&mut self, base: u32) -> Result<u32> {
        let mut acc: u32 = 0;
        while self.peek()? == 0 {
            acc += 0xFF;
            self.pos += 1;
        }
        let b = self.byte()? as u32;
        Ok(acc + base + b)
    }

    /// Copy `length` bytes from `src_off`, which may lie inside the region
    /// already written (overlapping matches are copied byte by byte).
    fn copy_match(&mut self, src_off: usize, length: usize) -> Result<()> {
        if src_off > self.out.len() {
            bail!(
                "lz77: back-reference before start of output ({} > {})",
                src_off,
                self.out.len()
            );
        }
        self.out.reserve(length);
        for i in 0..length {
            let b = self.out[src_off + i];
            self.out.push(b);
        }
        Ok(())
    }

    /// A back-reference distance is measured back from the output cursor.
    fn back(&self, distance: usize) -> Result<usize> {
        self.out
            .len()
            .checked_sub(distance)
            .ok_or_else(|| anyhow!("lz77: back-reference underflow (distance {distance})"))
    }

    /// The selector byte at `0x4c5616`: the byte two positions back in the
    /// input, which the original reads as `[eax - 2]`.
    fn selector(&self) -> u8 {
        if self.pos < 2 {
            0
        } else {
            self.src[self.pos - 2] & 3
        }
    }

    fn run(&mut self) -> Result<()> {
        let mut state = State::LiteralControl;

        // The first byte is read before the loop, mirroring the entry
        // sequence at `0x4c558c`.
        let first = self.byte()?;
        if first > 0x11 {
            // `0x4c5593`: copy `C - 0x11` literals first.
            let n = (first - 0x11) as usize;
            self.literals(n)?;
            if n < 4 {
                // `0x4c5620` falls through to `0x4c5629`: read a control
                // byte and dispatch straight to the match table.
                let c = self.byte()?;
                if !self.dispatch_match(c)? {
                    return Ok(());
                }
                state = State::AfterMatch;
            } else {
                // `0x4c55a1` then `0x4c55e6`.
                state = State::AfterLiteralRun;
            }
        } else {
            // `0x4c55ac`: the first byte is itself the control byte.
            if first >= 0x10 {
                if !self.dispatch_match(first)? {
                    return Ok(());
                }
                state = State::AfterMatch;
            } else {
                self.literal_run(first)?;
                state = State::AfterLiteralRun;
            }
        }

        loop {
            match state {
                State::LiteralControl => {
                    let c = self.byte()?;
                    if c >= 0x10 {
                        if !self.dispatch_match(c)? {
                            return Ok(());
                        }
                        state = State::AfterMatch;
                    } else {
                        self.literal_run(c)?;
                        state = State::AfterLiteralRun;
                    }
                }
                State::AfterLiteralRun => {
                    let c = self.byte()?;
                    if c >= 0x10 {
                        if !self.dispatch_match(c)? {
                            return Ok(());
                        }
                        state = State::AfterMatch;
                    } else {
                        // `0x4c55ef`: the special 3-byte match.
                        let o0 = self.byte()? as usize;
                        let distance = (o0 << 2) + (c as usize >> 2) + 0x801;
                        let src_off = self.back(distance)?;
                        self.copy_match(src_off, 3)?;
                        state = State::AfterMatch;
                    }
                }
                State::AfterMatch => {
                    // `0x4c5616`
                    let sel = self.selector();
                    if sel == 0 {
                        // `0x4c55ac`
                        state = State::LiteralControl;
                    } else {
                        // `0x4c5620`: copy `sel` literals, then `0x4c5629`
                        // reads a control byte and dispatches to a match.
                        self.literals(sel as usize)?;
                        let c = self.byte()?;
                        if !self.dispatch_match(c)? {
                            return Ok(());
                        }
                        state = State::AfterMatch;
                    }
                }
            }
        }
    }

    /// The literal run at `0x4c55b5`: three bytes are always copied, then
    /// `len` more.
    fn literal_run(&mut self, c: u8) -> Result<()> {
        let len = if c == 0 {
            self.extended_len(0x0F)?
        } else {
            c as u32
        };
        self.literals(3 + len as usize)
    }

    /// The match table at `0x4c562d`.
    ///
    /// Returns `false` when the end-of-stream marker was reached.
    fn dispatch_match(&mut self, c: u8) -> Result<bool> {
        if c >= 0x40 {
            // `0x4c5632`: 1-byte offset.
            let o = self.byte()? as usize;
            let distance = ((c as usize >> 2) & 7) + (o << 3) + 1;
            let length = (c as usize >> 5) + 1;
            let src_off = self.back(distance)?;
            self.copy_match(src_off, length)?;
        } else if c >= 0x20 {
            // `0x4c564e`: 2-byte offset.
            let mut length = (c & 0x1F) as u32;
            if length == 0 {
                length = self.extended_len(0x1F)?;
            }
            let o0 = self.byte()? as usize;
            let o1 = self.byte()? as usize;
            let distance = (o0 >> 2) + (o1 << 6) + 1;
            let src_off = self.back(distance)?;
            self.copy_match(src_off, length as usize + 2)?;
        } else if c >= 0x10 {
            // `0x4c5689`: 3-byte offset with a 16 KiB or 32 KiB window.
            let mut length = (c & 7) as u32;
            if length == 0 {
                length = self.extended_len(0x07)?;
            }
            let o0 = self.byte()? as usize;
            let o1 = self.byte()? as usize;
            let base = (o0 >> 2) + (o1 << 6);
            if base == 0 && c & 8 == 0 {
                // `0x4c56bd`: end-of-stream marker.
                return Ok(false);
            }
            // `0x4c56c1` subtracts a further `0x4000`.
            let distance = base + 0x4000 + if c & 8 != 0 { 0x4000 } else { 0 };
            let src_off = self.back(distance)?;
            self.copy_match(src_off, length as usize + 2)?;
        } else {
            // `0x4c56e6`: a 2-byte match with a short distance.
            let o0 = self.byte()? as usize;
            let distance = (o0 << 2) + (c as usize >> 2) + 1;
            let src_off = self.back(distance)?;
            self.copy_match(src_off, 2)?;
        }
        Ok(true)
    }
}
