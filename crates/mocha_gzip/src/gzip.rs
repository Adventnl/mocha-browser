//! gzip member framing (RFC 1952): header parsing, trailer verification, and
//! a stored-block-only encoder for tests/tools.

use mocha_error::{MochaError, MochaResult};

use crate::adler32::adler32;
use crate::crc32::crc32;
use crate::inflate::inflate_with_consumed;

fn err(message: impl Into<String>) -> MochaError {
    MochaError::Decompression(message.into())
}

const FLAG_FHCRC: u8 = 1 << 1;
const FLAG_FEXTRA: u8 = 1 << 2;
const FLAG_FNAME: u8 = 1 << 3;
const FLAG_FCOMMENT: u8 = 1 << 4;
const FLAG_RESERVED: u8 = 0xE0;

/// Decompress one complete gzip member, verifying the CRC-32 and length
/// trailer. Multi-member files and trailing garbage are clear errors (HTTP
/// `Content-Encoding: gzip` bodies are single members).
pub fn gzip_decompress(bytes: &[u8]) -> MochaResult<Vec<u8>> {
    let deflate_start = parse_header(bytes)?;
    let (output, deflate_length) = inflate_with_consumed(&bytes[deflate_start..])?;

    let trailer_start = deflate_start + deflate_length;
    let trailer: &[u8; 8] = bytes
        .get(trailer_start..trailer_start + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| err("gzip member is missing its 8-byte trailer"))?;
    if trailer_start + 8 != bytes.len() {
        return Err(err("unexpected trailing data after the gzip member"));
    }

    let expected_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let expected_size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    let actual_crc = crc32(&output);
    if actual_crc != expected_crc {
        return Err(err(format!(
            "gzip CRC-32 mismatch (expected {expected_crc:#010x}, got {actual_crc:#010x})"
        )));
    }
    if output.len() as u32 != expected_size {
        return Err(err("gzip uncompressed-size (ISIZE) mismatch"));
    }
    Ok(output)
}

/// Validate the gzip header and return the offset where the DEFLATE data starts.
fn parse_header(bytes: &[u8]) -> MochaResult<usize> {
    let truncated = || err("truncated gzip header");
    if bytes.len() < 10 {
        return Err(truncated());
    }
    if bytes[0] != 0x1F || bytes[1] != 0x8B {
        return Err(err("not gzip data (bad magic bytes)"));
    }
    if bytes[2] != 8 {
        return Err(err(format!(
            "unsupported gzip compression method {} (only DEFLATE/8)",
            bytes[2]
        )));
    }
    let flags = bytes[3];
    if flags & FLAG_RESERVED != 0 {
        return Err(err("gzip header sets reserved flag bits"));
    }
    // Skip MTIME (4), XFL (1), OS (1).
    let mut offset = 10;

    if flags & FLAG_FEXTRA != 0 {
        let length_bytes = bytes.get(offset..offset + 2).ok_or_else(truncated)?;
        let length = u16::from_le_bytes([length_bytes[0], length_bytes[1]]) as usize;
        offset += 2 + length;
        if bytes.len() < offset {
            return Err(truncated());
        }
    }
    for flag in [FLAG_FNAME, FLAG_FCOMMENT] {
        if flags & flag != 0 {
            let terminator = bytes[offset..]
                .iter()
                .position(|&byte| byte == 0)
                .ok_or_else(truncated)?;
            offset += terminator + 1;
        }
    }
    if flags & FLAG_FHCRC != 0 {
        let stored = bytes.get(offset..offset + 2).ok_or_else(truncated)?;
        let stored = u16::from_le_bytes([stored[0], stored[1]]);
        let computed = (crc32(&bytes[..offset]) & 0xFFFF) as u16;
        if stored != computed {
            return Err(err("gzip header CRC (FHCRC) mismatch"));
        }
        offset += 2;
    }
    Ok(offset)
}

/// Encode `bytes` as a valid gzip member using only *stored* (uncompressed)
/// DEFLATE blocks. No compression is performed; this exists so tests and the
/// localhost test server can produce real gzip bodies without a compressor.
pub fn gzip_compress_stored(bytes: &[u8]) -> Vec<u8> {
    let mut output = vec![
        0x1F, 0x8B, // magic
        8,    // CM = DEFLATE
        0,    // FLG: none
        0, 0, 0, 0,   // MTIME: unset
        0,   // XFL
        255, // OS: unknown
    ];

    let mut chunks = bytes.chunks(0xFFFF).peekable();
    if chunks.peek().is_none() {
        // Empty input still needs one (final, empty) stored block.
        output.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        let final_block = chunks.peek().is_none();
        output.push(final_block as u8); // BFINAL + BTYPE=00, byte-aligned
        let length = chunk.len() as u16;
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&(!length).to_le_bytes());
        output.extend_from_slice(chunk);
    }

    output.extend_from_slice(&crc32(bytes).to_le_bytes());
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output
}

/// Decompress a zlib (RFC 1950) stream — the wire format for HTTP
/// `Content-Encoding: deflate`. The 2-byte header (CMF + FLG) is validated and
/// the trailing big-endian Adler-32 is verified after running the shared raw
/// DEFLATE decoder.
///
/// Some servers mislabel a *raw* DEFLATE stream (no zlib wrapper) as `deflate`.
/// If the leading bytes are not a valid zlib header, fall back to treating the
/// whole body as raw DEFLATE rather than rejecting it.
pub fn zlib_decompress(bytes: &[u8]) -> MochaResult<Vec<u8>> {
    if is_zlib_header(bytes) {
        let (output, deflate_length) = inflate_with_consumed(&bytes[2..])?;
        let trailer_start = 2 + deflate_length;
        let trailer: &[u8; 4] = bytes
            .get(trailer_start..trailer_start + 4)
            .and_then(|slice| slice.try_into().ok())
            .ok_or_else(|| err("zlib stream is missing its 4-byte Adler-32 trailer"))?;
        let expected = u32::from_be_bytes(*trailer);
        let actual = adler32(&output);
        if actual != expected {
            return Err(err(format!(
                "zlib Adler-32 mismatch (expected {expected:#010x}, got {actual:#010x})"
            )));
        }
        Ok(output)
    } else {
        // Raw DEFLATE fallback for servers that omit the zlib wrapper.
        crate::inflate::inflate(bytes)
    }
}

/// Whether `bytes` begins with a valid RFC 1950 zlib header: compression
/// method 8 (DEFLATE), no preset dictionary (FDICT clear), and the 16-bit
/// header value a multiple of 31 (the FCHECK constraint).
fn is_zlib_header(bytes: &[u8]) -> bool {
    let [cmf, flg] = match bytes.first_chunk::<2>() {
        Some(header) => *header,
        None => return false,
    };
    const FDICT: u8 = 1 << 5;
    cmf & 0x0F == 8 && flg & FDICT == 0 && (u16::from(cmf) << 8 | u16::from(flg)) % 31 == 0
}

/// Encode `bytes` as a valid zlib (RFC 1950) stream using only *stored* DEFLATE
/// blocks (mirrors [`gzip_compress_stored`]; for tests and the test server).
pub fn zlib_compress_stored(bytes: &[u8]) -> Vec<u8> {
    // CMF = 0x78 (CM=8, CINFO=7), FLG chosen so the 16-bit header % 31 == 0.
    let mut output = vec![0x78, 0x01];
    let mut chunks = bytes.chunks(0xFFFF).peekable();
    if chunks.peek().is_none() {
        output.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        let final_block = chunks.peek().is_none();
        output.push(final_block as u8); // BFINAL + BTYPE=00, byte-aligned
        let length = chunk.len() as u16;
        output.extend_from_slice(&length.to_le_bytes());
        output.extend_from_slice(&(!length).to_le_bytes());
        output.extend_from_slice(chunk);
    }
    output.extend_from_slice(&adler32(bytes).to_be_bytes());
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_encoder_round_trips() {
        for input in [
            &b""[..],
            b"a",
            b"hello gzip world",
            &[0_u8, 1, 2, 255, 254, 253],
        ] {
            let encoded = gzip_compress_stored(input);
            assert_eq!(gzip_decompress(&encoded).unwrap(), input);
        }
    }

    #[test]
    fn stored_encoder_splits_large_input_into_blocks() {
        let input: Vec<u8> = (0..200_000_usize).map(|index| index as u8).collect();
        let encoded = gzip_compress_stored(&input);
        assert_eq!(gzip_decompress(&encoded).unwrap(), input);
    }

    #[test]
    fn real_gzip_fixture_with_filename_decodes() {
        // Produced by GNU gzip (keeps the original file name in the header).
        let original = include_bytes!("../testdata/hello.txt");
        let compressed = include_bytes!("../testdata/hello.txt.gz");
        assert_eq!(gzip_decompress(compressed).unwrap(), original);
    }

    #[test]
    fn real_gzip_fixture_dynamic_huffman_decodes() {
        // A few KB of text: GNU gzip emits dynamic-Huffman blocks for this.
        let original = include_bytes!("../testdata/lorem.txt");
        let compressed = include_bytes!("../testdata/lorem.txt.gz");
        assert_eq!(gzip_decompress(compressed).unwrap(), original);
    }

    #[test]
    fn bad_magic_errors() {
        let error = gzip_decompress(b"PK\x03\x04not gzip").unwrap_err();
        assert!(error.to_string().contains("bad magic"));
    }

    #[test]
    fn truncated_member_errors() {
        let encoded = gzip_compress_stored(b"hello gzip world");
        let error = gzip_decompress(&encoded[..encoded.len() - 4]).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
    }

    #[test]
    fn corrupted_crc_errors() {
        let mut encoded = gzip_compress_stored(b"hello gzip world");
        let length = encoded.len();
        encoded[length - 8] ^= 0xFF; // flip a CRC byte
        let error = gzip_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("CRC-32 mismatch"));
    }

    #[test]
    fn corrupted_isize_errors() {
        let mut encoded = gzip_compress_stored(b"hello gzip world");
        let length = encoded.len();
        encoded[length - 1] ^= 0xFF; // flip an ISIZE byte
        let error = gzip_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("ISIZE"));
    }

    #[test]
    fn trailing_garbage_errors() {
        let mut encoded = gzip_compress_stored(b"hello");
        encoded.extend_from_slice(b"junk");
        let error = gzip_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("trailing data"));
    }

    #[test]
    fn reserved_flag_bits_error() {
        let mut encoded = gzip_compress_stored(b"hello");
        encoded[3] = 0x80;
        let error = gzip_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("reserved flag"));
    }

    #[test]
    fn non_deflate_method_errors() {
        let mut encoded = gzip_compress_stored(b"hello");
        encoded[2] = 9;
        let error = gzip_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("compression method"));
    }

    #[test]
    fn zlib_stored_encoder_round_trips() {
        for input in [
            &b""[..],
            b"a",
            b"hello zlib world",
            &[0_u8, 1, 2, 255, 254, 253],
        ] {
            let encoded = zlib_compress_stored(input);
            assert_eq!(zlib_decompress(&encoded).unwrap(), input);
        }
    }

    #[test]
    fn zlib_stored_encoder_splits_large_input_into_blocks() {
        let input: Vec<u8> = (0..200_000_usize).map(|index| index as u8).collect();
        let encoded = zlib_compress_stored(&input);
        assert_eq!(zlib_decompress(&encoded).unwrap(), input);
    }

    #[test]
    fn raw_deflate_without_zlib_header_is_tolerated() {
        // A bare DEFLATE stream (gzip body minus its 10-byte header and 8-byte
        // trailer) whose first bytes are not a valid zlib header.
        let gz = gzip_compress_stored(b"raw deflate fallback");
        let raw = &gz[10..gz.len() - 8];
        assert!(!is_zlib_header(raw));
        assert_eq!(zlib_decompress(raw).unwrap(), b"raw deflate fallback");
    }

    #[test]
    fn zlib_corrupted_adler32_errors() {
        let mut encoded = zlib_compress_stored(b"hello zlib world");
        let length = encoded.len();
        encoded[length - 1] ^= 0xFF; // flip an Adler-32 byte
        let error = zlib_decompress(&encoded).unwrap_err();
        assert!(error.to_string().contains("Adler-32 mismatch"));
    }

    #[test]
    fn zlib_truncated_trailer_errors() {
        let encoded = zlib_compress_stored(b"hello zlib world");
        let error = zlib_decompress(&encoded[..encoded.len() - 2]).unwrap_err();
        assert!(matches!(error, MochaError::Decompression(_)));
    }
}
