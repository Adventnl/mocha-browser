//! Adler-32 (RFC 1950 §2.2), the zlib stream checksum: two 16-bit sums taken
//! modulo 65521 (the largest prime below 2^16).

/// The largest prime smaller than 65536; both Adler sums are reduced modulo it.
const MOD_ADLER: u32 = 65521;

/// The Adler-32 of `bytes` as used by the zlib trailer (RFC 1950).
pub fn adler32(bytes: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in bytes {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::adler32;

    #[test]
    fn known_vectors() {
        // RFC 1950 worked-out values.
        assert_eq!(adler32(b""), 0x0000_0001);
        assert_eq!(adler32(b"a"), 0x0062_0062);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }
}
