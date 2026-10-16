//! CRC-32 (IEEE, as `zlib.crc32`): manifest CRCs, style pack headers, PNG chunks.

const fn table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static TABLE: [u32; 256] = table();

/// `zlib.crc32(data)`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

/// `zlib.crc32(data, start)`: continue a running CRC.
pub fn crc32_update(start: u32, data: &[u8]) -> u32 {
    let mut c = !start;
    for b in data {
        c = TABLE[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// `f"{zlib.crc32(data):08x}"`.
pub fn crc32_hex(data: &[u8]) -> String {
    format!("{:08x}", crc32(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32_update(crc32(b"1234"), b"56789"), 0xCBF4_3926);
        assert_eq!(crc32_hex(b"a"), "e8b7be43");
    }
}
