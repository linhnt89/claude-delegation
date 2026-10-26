//! SHA-256 for kit content fingerprints (FIPS 180-4, no external dependencies).

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

// Working variables use the names in FIPS 180-4, section 6.2.2.
#[allow(clippy::many_single_char_names)]
pub fn digest(input: &[u8]) -> [u8; 32] {
    let padded_len = (input.len() + 9).div_ceil(64) * 64;
    let bit_len = (input.len() as u64).wrapping_mul(8).to_be_bytes();
    let mut state = [
        0x6a09_e667_u32,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut offset = 0;
    while offset < padded_len {
        let mut w = [0_u32; 64];
        let mut pos = 0;
        while pos < 64 {
            let index = offset + pos;
            let byte = if index < input.len() {
                input[index]
            } else if index == input.len() {
                0x80
            } else if index >= padded_len - 8 {
                bit_len[index - (padded_len - 8)]
            } else {
                0
            };
            w[pos / 4] |= u32::from(byte) << (24 - (pos % 4) * 8);
            pos += 1;
        }
        let mut i = 16;
        while i < 64 {
            let x = w[i - 15];
            let y = w[i - 2];
            let s0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
            let s1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
            i += 1;
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        i = 0;
        while i < 64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
            i += 1;
        }
        let values = [a, b, c, d, e, f, g, h];
        i = 0;
        while i < 8 {
            state[i] = state[i].wrapping_add(values[i]);
            i += 1;
        }
        offset += 64;
    }
    let mut result = [0; 32];
    let mut i = 0;
    while i < 8 {
        let bytes = state[i].to_be_bytes();
        let mut j = 0;
        while j < 4 {
            result[i * 4 + j] = bytes[j];
            j += 1;
        }
        i += 1;
    }
    result
}

pub fn hex_bytes(digest: [u8; 32]) -> [u8; 64] {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = [0; 64];
    let mut i = 0;
    while i < 32 {
        output[i * 2] = DIGITS[(digest[i] >> 4) as usize];
        output[i * 2 + 1] = DIGITS[(digest[i] & 15) as usize];
        i += 1;
    }
    output
}

pub fn hex(input: &[u8]) -> String {
    String::from_utf8_lossy(&hex_bytes(digest(input))).into_owned()
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn padding_boundaries() {
        for (length, expected) in [
            (
                55,
                "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
            ),
            (
                56,
                "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
            ),
            (
                63,
                "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
            ),
            (
                64,
                "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
            ),
            (
                65,
                "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0",
            ),
        ] {
            assert_eq!(hex(&vec![b'a'; length]), expected);
        }
    }

    #[test]
    fn known_vectors() {
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        assert_eq!(
            hex(&vec![b'a'; 1_048_576]),
            "9bc1b2a288b26af7257a36277ae3816a7d4f16e89c1e7e77d0a5c48bad62b360"
        );
    }
}
