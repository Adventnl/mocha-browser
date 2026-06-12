//! A from-scratch DEFLATE (RFC 1951) decompressor.
//!
//! Supports all three block types — stored, fixed-Huffman, and
//! dynamic-Huffman — with canonical-Huffman decoding (the bit-at-a-time
//! `count`/`symbol` walk described in RFC 1951 §3.2.2). Malformed streams fail
//! with [`MochaError::Decompression`]; output larger than
//! [`MAX_OUTPUT_BYTES`] is rejected rather than risked.

use mocha_error::{MochaError, MochaResult};

/// Decompressed output above this size aborts decoding. This protects the
/// browser from "zip bomb" responses: DEFLATE can expand ~1032:1, so a small
/// body could otherwise allocate unbounded memory.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

fn err(message: impl Into<String>) -> MochaError {
    MochaError::Decompression(message.into())
}

/// Inflate a raw DEFLATE stream (no gzip/zlib wrapper).
pub fn inflate(data: &[u8]) -> MochaResult<Vec<u8>> {
    inflate_with_consumed(data).map(|(output, _)| output)
}

/// Inflate a raw DEFLATE stream, also returning how many input bytes the
/// stream occupied (the gzip decoder needs this to find the member trailer).
pub(crate) fn inflate_with_consumed(data: &[u8]) -> MochaResult<(Vec<u8>, usize)> {
    let mut reader = BitReader::new(data);
    let mut output = Vec::new();

    loop {
        let final_block = reader.take(1)? == 1;
        let block_type = reader.take(2)?;
        match block_type {
            0 => inflate_stored_block(&mut reader, &mut output)?,
            1 => {
                let (literal, distance) = fixed_tables();
                inflate_compressed_block(&mut reader, &mut output, &literal, &distance)?
            }
            2 => {
                let (literal, distance) = read_dynamic_tables(&mut reader)?;
                inflate_compressed_block(&mut reader, &mut output, &literal, &distance)?
            }
            _ => return Err(err("reserved deflate block type 3")),
        }
        if final_block {
            break;
        }
    }

    reader.align_to_byte();
    Ok((output, reader.bytes_consumed()))
}

/// An LSB-first bit reader over a byte slice (DEFLATE's bit order).
struct BitReader<'a> {
    bytes: &'a [u8],
    /// Index of the next byte to load into the bit buffer.
    position: usize,
    bit_buffer: u32,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> BitReader<'a> {
        BitReader {
            bytes,
            position: 0,
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// Read `count` bits (0..=16), LSB first.
    fn take(&mut self, count: u32) -> MochaResult<u32> {
        debug_assert!(count <= 16);
        while self.bit_count < count {
            let byte = *self
                .bytes
                .get(self.position)
                .ok_or_else(|| err("unexpected end of deflate stream"))?;
            self.bit_buffer |= (byte as u32) << self.bit_count;
            self.bit_count += 8;
            self.position += 1;
        }
        let value = self.bit_buffer & ((1_u32 << count) - 1);
        self.bit_buffer >>= count;
        self.bit_count -= count;
        Ok(value)
    }

    /// Discard buffered bits up to the next byte boundary.
    fn align_to_byte(&mut self) {
        let partial = self.bit_count % 8;
        self.bit_buffer >>= partial;
        self.bit_count -= partial;
    }

    /// Copy `length` whole bytes (only valid right after `align_to_byte`).
    fn take_bytes(&mut self, length: usize, output: &mut Vec<u8>) -> MochaResult<()> {
        debug_assert_eq!(self.bit_count % 8, 0);
        // Drain any whole bytes still sitting in the bit buffer first.
        let mut remaining = length;
        while remaining > 0 && self.bit_count >= 8 {
            output.push((self.bit_buffer & 0xFF) as u8);
            self.bit_buffer >>= 8;
            self.bit_count -= 8;
            remaining -= 1;
        }
        let end = self
            .position
            .checked_add(remaining)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| err("stored deflate block is truncated"))?;
        output.extend_from_slice(&self.bytes[self.position..end]);
        self.position = end;
        Ok(())
    }

    /// How many input bytes have been consumed (call after `align_to_byte`).
    fn bytes_consumed(&self) -> usize {
        self.position - (self.bit_count / 8) as usize
    }
}

/// A canonical Huffman decoding table: `count[len]` is how many codes have
/// length `len`; `symbols` lists symbols ordered by (code length, symbol).
struct Huffman {
    count: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    /// Build from per-symbol code lengths (0 = unused). Rejects over-subscribed
    /// length sets; incomplete sets are allowed (decoding errors if a gap code
    /// is actually encountered), which matches RFC 1951's single-code cases.
    fn new(lengths: &[u8]) -> MochaResult<Huffman> {
        let mut count = [0_u16; 16];
        for &length in lengths {
            if length > 15 {
                return Err(err("huffman code length exceeds 15"));
            }
            count[length as usize] += 1;
        }

        let mut remaining = 1_i32;
        for &length_count in count.iter().skip(1) {
            remaining <<= 1;
            remaining -= length_count as i32;
            if remaining < 0 {
                return Err(err("over-subscribed huffman code lengths"));
            }
        }

        // offsets[len] = index of the first symbol with that code length.
        let mut offsets = [0_usize; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + count[length] as usize;
        }
        let mut symbols = vec![0_u16; lengths.len() - count[0] as usize];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[offsets[length as usize]] = symbol as u16;
                offsets[length as usize] += 1;
            }
        }

        Ok(Huffman { count, symbols })
    }

    /// Decode one symbol, reading bits MSB-of-code-first per RFC 1951.
    fn decode(&self, reader: &mut BitReader) -> MochaResult<u16> {
        let mut code = 0_i32;
        let mut first = 0_i32;
        let mut index = 0_i32;
        for length in 1..=15 {
            code |= reader.take(1)? as i32;
            let count = self.count[length] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(err("invalid huffman code in deflate stream"))
    }
}

/// Stored (uncompressed) block: byte-aligned LEN/NLEN then raw bytes.
fn inflate_stored_block(reader: &mut BitReader, output: &mut Vec<u8>) -> MochaResult<()> {
    reader.align_to_byte();
    let len = reader.take(16)?;
    let nlen = reader.take(16)?;
    if len != !nlen & 0xFFFF {
        return Err(err("stored deflate block LEN/NLEN mismatch"));
    }
    if output.len() + len as usize > MAX_OUTPUT_BYTES {
        return Err(output_limit_error());
    }
    reader.take_bytes(len as usize, output)
}

/// Extra bits and base values for length codes 257..=285 (RFC 1951 §3.2.5).
const LENGTH_EXTRA: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const LENGTH_BASE: [u32; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

/// Extra bits and base values for distance codes 0..=29.
const DISTANCE_EXTRA: [u32; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const DISTANCE_BASE: [u32; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// Decode literal/length + distance symbols until end-of-block (symbol 256).
fn inflate_compressed_block(
    reader: &mut BitReader,
    output: &mut Vec<u8>,
    literal: &Huffman,
    distance: &Huffman,
) -> MochaResult<()> {
    loop {
        let symbol = literal.decode(reader)?;
        match symbol {
            0..=255 => {
                if output.len() >= MAX_OUTPUT_BYTES {
                    return Err(output_limit_error());
                }
                output.push(symbol as u8);
            }
            256 => return Ok(()),
            257..=285 => {
                let index = (symbol - 257) as usize;
                let length = LENGTH_BASE[index] + reader.take(LENGTH_EXTRA[index])?;

                let distance_symbol = distance.decode(reader)?;
                if distance_symbol > 29 {
                    return Err(err("invalid deflate distance code"));
                }
                let index = distance_symbol as usize;
                let distance =
                    (DISTANCE_BASE[index] + reader.take(DISTANCE_EXTRA[index])?) as usize;
                if distance > output.len() {
                    return Err(err("deflate back-reference reaches before output start"));
                }
                if output.len() + length as usize > MAX_OUTPUT_BYTES {
                    return Err(output_limit_error());
                }
                // Byte-at-a-time copy: distances shorter than the length
                // deliberately repeat the just-written bytes (RFC 1951 §3.2.3).
                let start = output.len() - distance;
                for offset in 0..length as usize {
                    let byte = output[start + offset];
                    output.push(byte);
                }
            }
            _ => return Err(err("invalid deflate literal/length code")),
        }
    }
}

/// The fixed-Huffman tables of RFC 1951 §3.2.6.
fn fixed_tables() -> (Huffman, Huffman) {
    let mut literal_lengths = [0_u8; 288];
    literal_lengths[0..144].fill(8);
    literal_lengths[144..256].fill(9);
    literal_lengths[256..280].fill(7);
    literal_lengths[280..288].fill(8);
    let distance_lengths = [5_u8; 30];
    // Both tables are valid by construction.
    let literal = Huffman::new(&literal_lengths).expect("fixed literal table");
    let distance = Huffman::new(&distance_lengths).expect("fixed distance table");
    (literal, distance)
}

/// The order in which code-length code lengths are stored (RFC 1951 §3.2.7).
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Read the dynamic-Huffman table definitions that prefix a type-2 block.
fn read_dynamic_tables(reader: &mut BitReader) -> MochaResult<(Huffman, Huffman)> {
    let literal_count = reader.take(5)? as usize + 257;
    let distance_count = reader.take(5)? as usize + 1;
    let code_length_count = reader.take(4)? as usize + 4;
    if literal_count > 286 || distance_count > 30 {
        return Err(err("dynamic deflate block declares too many codes"));
    }

    let mut code_length_lengths = [0_u8; 19];
    for &index in CODE_LENGTH_ORDER.iter().take(code_length_count) {
        code_length_lengths[index] = reader.take(3)? as u8;
    }
    let code_length_table = Huffman::new(&code_length_lengths)?;

    // Literal/length and distance code lengths share one encoded sequence.
    let total = literal_count + distance_count;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let symbol = code_length_table.decode(reader)?;
        match symbol {
            0..=15 => lengths.push(symbol as u8),
            16 => {
                let &previous = lengths
                    .last()
                    .ok_or_else(|| err("dynamic deflate block repeats with no previous length"))?;
                let repeat = 3 + reader.take(2)? as usize;
                lengths.resize(lengths.len() + repeat, previous);
            }
            17 => {
                let repeat = 3 + reader.take(3)? as usize;
                lengths.resize(lengths.len() + repeat, 0);
            }
            18 => {
                let repeat = 11 + reader.take(7)? as usize;
                lengths.resize(lengths.len() + repeat, 0);
            }
            _ => return Err(err("invalid code-length symbol in dynamic deflate block")),
        }
    }
    if lengths.len() != total {
        return Err(err("dynamic deflate code lengths overflow their count"));
    }
    if lengths[256] == 0 {
        return Err(err("dynamic deflate block has no end-of-block code"));
    }

    let literal = Huffman::new(&lengths[..literal_count])?;
    let distance = Huffman::new(&lengths[literal_count..])?;
    Ok((literal, distance))
}

fn output_limit_error() -> MochaError {
    err(format!(
        "decompressed output exceeds the {MAX_OUTPUT_BYTES}-byte limit"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build raw DEFLATE streams bit-by-bit for tests. Header fields are
    /// written LSB-first; Huffman codes are written MSB-of-code-first,
    /// matching RFC 1951 §3.1.1.
    pub(crate) struct BitWriter {
        pub bytes: Vec<u8>,
        bit: u32,
    }

    impl BitWriter {
        pub fn new() -> BitWriter {
            BitWriter {
                bytes: Vec::new(),
                bit: 0,
            }
        }

        pub fn push_bits(&mut self, value: u32, count: u32) {
            for index in 0..count {
                self.push_bit((value >> index) & 1);
            }
        }

        pub fn push_code(&mut self, code: u32, length: u32) {
            for index in (0..length).rev() {
                self.push_bit((code >> index) & 1);
            }
        }

        fn push_bit(&mut self, bit: u32) {
            if self.bit == 0 {
                self.bytes.push(0);
            }
            let last = self.bytes.last_mut().unwrap();
            *last |= (bit as u8) << self.bit;
            self.bit = (self.bit + 1) % 8;
        }
    }

    /// Append one fixed-Huffman literal byte.
    pub(crate) fn push_fixed_literal(writer: &mut BitWriter, byte: u8) {
        if byte < 144 {
            writer.push_code(0x30 + byte as u32, 8);
        } else {
            writer.push_code(0x190 + (byte as u32 - 144), 9);
        }
    }

    /// Append a fixed-Huffman length/distance pair (no extra bits cases only).
    pub(crate) fn push_fixed_match(
        writer: &mut BitWriter,
        length_symbol: u16,
        distance_symbol: u16,
    ) {
        assert!((257..=285).contains(&length_symbol));
        assert_eq!(LENGTH_EXTRA[(length_symbol - 257) as usize], 0);
        assert_eq!(DISTANCE_EXTRA[distance_symbol as usize], 0);
        if length_symbol < 280 {
            writer.push_code((length_symbol - 256) as u32, 7);
        } else {
            writer.push_code(0xC0 + (length_symbol as u32 - 280), 8);
        }
        writer.push_code(distance_symbol as u32, 5);
    }

    pub(crate) fn push_fixed_end_of_block(writer: &mut BitWriter) {
        writer.push_code(0, 7);
    }

    #[test]
    fn stored_block_round_trips() {
        // BFINAL=1, BTYPE=00, align, LEN/NLEN, raw bytes.
        let mut stream = vec![0x01, 0x05, 0x00, 0xFA, 0xFF];
        stream.extend_from_slice(b"mocha");
        assert_eq!(inflate(&stream).unwrap(), b"mocha");
    }

    #[test]
    fn stored_block_len_nlen_mismatch_errors() {
        let mut stream = vec![0x01, 0x05, 0x00, 0x00, 0x00];
        stream.extend_from_slice(b"mocha");
        let error = inflate(&stream).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
        assert!(error.to_string().contains("LEN/NLEN"));
    }

    #[test]
    fn fixed_huffman_literals_decode() {
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1); // BFINAL
        writer.push_bits(1, 2); // BTYPE=01 fixed
        for &byte in b"hi mocha" {
            push_fixed_literal(&mut writer, byte);
        }
        push_fixed_end_of_block(&mut writer);
        assert_eq!(inflate(&writer.bytes).unwrap(), b"hi mocha");
    }

    #[test]
    fn fixed_huffman_high_literals_decode() {
        // Literals >= 144 use the 9-bit fixed codes.
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1);
        writer.push_bits(1, 2);
        for byte in [200_u8, 255, 144] {
            push_fixed_literal(&mut writer, byte);
        }
        push_fixed_end_of_block(&mut writer);
        assert_eq!(inflate(&writer.bytes).unwrap(), vec![200, 255, 144]);
    }

    #[test]
    fn fixed_huffman_back_reference_decodes() {
        // "abc" + <length 6, distance 3> = "abcabcabc".
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1);
        writer.push_bits(1, 2);
        for &byte in b"abc" {
            push_fixed_literal(&mut writer, byte);
        }
        push_fixed_match(&mut writer, 260, 2); // length base 6, distance base 3
        push_fixed_end_of_block(&mut writer);
        assert_eq!(inflate(&writer.bytes).unwrap(), b"abcabcabc");
    }

    #[test]
    fn overlapping_back_reference_repeats_byte() {
        // "a" + <length 8, distance 1> = "aaaaaaaaa".
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1);
        writer.push_bits(1, 2);
        push_fixed_literal(&mut writer, b'a');
        push_fixed_match(&mut writer, 262, 0); // length base 8, distance 1
        push_fixed_end_of_block(&mut writer);
        assert_eq!(inflate(&writer.bytes).unwrap(), b"aaaaaaaaa");
    }

    #[test]
    fn distance_before_output_start_errors() {
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1);
        writer.push_bits(1, 2);
        push_fixed_literal(&mut writer, b'a');
        push_fixed_match(&mut writer, 260, 2); // distance 3 > 1 byte of output
        push_fixed_end_of_block(&mut writer);
        let error = inflate(&writer.bytes).unwrap_err();
        assert!(error.to_string().contains("before output start"));
    }

    #[test]
    fn reserved_block_type_errors() {
        // BFINAL=1, BTYPE=11.
        let error = inflate(&[0x07]).unwrap_err();
        assert!(error.to_string().contains("reserved deflate block type"));
    }

    #[test]
    fn truncated_stream_errors() {
        let error = inflate(&[0x01, 0x05]).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
    }

    #[test]
    fn empty_input_errors() {
        assert!(inflate(&[]).is_err());
    }

    #[test]
    fn multiple_blocks_concatenate() {
        // A non-final stored block followed by a final fixed block.
        let mut writer = BitWriter::new();
        writer.push_bits(0, 1); // BFINAL=0
        writer.push_bits(0, 2); // stored
        let mut stream = writer.bytes.clone();
        stream.extend_from_slice(&[0x02, 0x00, 0xFD, 0xFF]); // LEN=2, NLEN
        stream.extend_from_slice(b"ab");

        let mut tail = BitWriter::new();
        tail.push_bits(1, 1);
        tail.push_bits(1, 2);
        push_fixed_literal(&mut tail, b'c');
        push_fixed_end_of_block(&mut tail);
        stream.extend_from_slice(&tail.bytes);

        assert_eq!(inflate(&stream).unwrap(), b"abc");
    }

    #[test]
    fn output_limit_is_enforced() {
        // 'a' then ~260k copies of <length 258, distance 1> ≈ 67 MB > the cap.
        let mut writer = BitWriter::new();
        writer.push_bits(1, 1);
        writer.push_bits(1, 2);
        push_fixed_literal(&mut writer, b'a');
        for _ in 0..262_000 {
            push_fixed_match(&mut writer, 285, 0); // length 258, distance 1
        }
        push_fixed_end_of_block(&mut writer);
        let error = inflate(&writer.bytes).unwrap_err();
        assert!(error.to_string().contains("limit"));
    }

    #[test]
    fn over_subscribed_code_lengths_error() {
        // Four codes of length 1 are impossible.
        let error = match Huffman::new(&[1, 1, 1, 1]) {
            Err(error) => error,
            Ok(_) => panic!("expected an over-subscribed error"),
        };
        assert!(error.to_string().contains("over-subscribed"));
    }
}
