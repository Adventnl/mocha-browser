//! From-scratch gzip (RFC 1952) and DEFLATE (RFC 1951) decoding.
//!
//! `mocha_gzip` exists so that `mocha_net` can decode `Content-Encoding: gzip`
//! HTTP responses (Milestone 21) without a third-party compression library.
//! It implements the inflate side only: stored, fixed-Huffman, and
//! dynamic-Huffman DEFLATE blocks, the gzip member header/trailer (including
//! CRC-32 and length verification), and a deliberately tiny *stored-block*
//! gzip encoder used by tests and test servers (valid gzip, zero compression).
//!
//! Out of scope (clear errors, never silent): multi-member gzip files, zlib
//! (`Content-Encoding: deflate`) streams, brotli, zstd, and any real
//! compression. Decompressed output is capped at [`MAX_OUTPUT_BYTES`] so a
//! malicious "zip bomb" response cannot exhaust memory.

mod crc32;
mod gzip;
mod inflate;

pub use crc32::crc32;
pub use gzip::{gzip_compress_stored, gzip_decompress};
pub use inflate::{inflate, MAX_OUTPUT_BYTES};
