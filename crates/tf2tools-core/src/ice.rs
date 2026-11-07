//! ICE decryption for TF2's encrypted weapon scripts (`scripts/*.ctx`), which Valve encrypts
//! with 8-round ICE in ECB mode, leaving a trailing partial block in the clear.

/// The key TF2 encrypts its `.ctx` weapon scripts with.
pub(crate) const CTX_KEY: &[u8; 8] = b"E2NcUkG2";

const ROUNDS: usize = 8;

const SMOD: [[u32; 4]; 4] = [
    [333, 313, 505, 369],
    [379, 375, 319, 391],
    [361, 445, 451, 397],
    [397, 425, 395, 505],
];
const SXOR: [[u32; 4]; 4] = [
    [0x83, 0x85, 0x9b, 0xcd],
    [0xcc, 0xa7, 0xad, 0x41],
    [0x4b, 0x2e, 0xd4, 0x33],
    [0xea, 0xcb, 0x2e, 0x04],
];
const PBOX: [u32; 32] = [
    0x0000_0001,
    0x0000_0080,
    0x0000_0400,
    0x0000_2000,
    0x0008_0000,
    0x0020_0000,
    0x0100_0000,
    0x4000_0000,
    0x0000_0008,
    0x0000_0020,
    0x0000_0100,
    0x0000_4000,
    0x0001_0000,
    0x0080_0000,
    0x0400_0000,
    0x2000_0000,
    0x0000_0004,
    0x0000_0010,
    0x0000_0200,
    0x0000_8000,
    0x0002_0000,
    0x0040_0000,
    0x0800_0000,
    0x1000_0000,
    0x0000_0002,
    0x0000_0040,
    0x0000_0800,
    0x0000_1000,
    0x0004_0000,
    0x0010_0000,
    0x0200_0000,
    0x8000_0000,
];
const KEYROT: [usize; 8] = [0, 1, 2, 3, 2, 1, 3, 0];

/// Multiplies `a` and `b` in GF(2^8) reduced by the polynomial `m`.
fn gf_mul(mut a: u32, mut b: u32, m: u32) -> u32 {
    let mut r = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a <<= 1;
        b >>= 1;
        if a >= 256 {
            a ^= m;
        }
    }
    r
}

/// `b` raised to the seventh power in GF(2^8) mod `m`, the S-box core.
fn gf_exp7(b: u32, m: u32) -> u32 {
    if b == 0 {
        return 0;
    }
    let x = gf_mul(b, b, m);
    let x = gf_mul(b, x, m);
    let x = gf_mul(x, x, m);
    gf_mul(b, x, m)
}

fn permute(mut x: u32) -> u32 {
    let mut r = 0;
    let mut i = 0;
    while x != 0 {
        if x & 1 != 0 {
            r |= PBOX[i];
        }
        i += 1;
        x >>= 1;
    }
    r
}

/// The four 1024-entry S-boxes with the P-box folded in, built once.
fn sboxes() -> &'static [[u32; 1024]; 4] {
    static SBOX: std::sync::OnceLock<Box<[[u32; 1024]; 4]>> = std::sync::OnceLock::new();
    SBOX.get_or_init(|| {
        let mut sbox = Box::new([[0u32; 1024]; 4]);
        for i in 0..1024u32 {
            let col = (i >> 1) & 0xff;
            let row = ((i & 1) | ((i & 0x200) >> 8)) as usize;
            for (k, table) in sbox.iter_mut().enumerate() {
                table[i as usize] =
                    permute(gf_exp7(col ^ SXOR[k][row], SMOD[k][row]) << (24 - k * 8));
            }
        }
        sbox
    })
}

/// The round function: expand the 32-bit half to two 20-bit halves, key them, substitute.
fn round(p: u32, sk: &[u32; 3], sbox: &[[u32; 1024]; 4]) -> u32 {
    let tl = ((p >> 16) & 0x3ff) | (p.rotate_right(14) & 0xffc00);
    let tr = (p & 0x3ff) | ((p << 2) & 0xffc00);
    let mut al = sk[2] & (tl ^ tr);
    let mut ar = al ^ tr;
    al ^= tl;
    al ^= sk[0];
    ar ^= sk[1];
    sbox[0][(al >> 10) as usize]
        | sbox[1][(al & 0x3ff) as usize]
        | sbox[2][(ar >> 10) as usize]
        | sbox[3][(ar & 0x3ff) as usize]
}

/// The eight round subkeys, drawn bit by bit from the key's four 16-bit words.
fn schedule(key: &[u8; 8]) -> [[u32; 3]; ROUNDS] {
    let mut kb = [0u32; 4];
    for i in 0..4 {
        kb[3 - i] = (u32::from(key[i * 2]) << 8) | u32::from(key[i * 2 + 1]);
    }
    let mut ks = [[0u32; 3]; ROUNDS];
    for (sk, &kr) in ks.iter_mut().zip(KEYROT.iter()) {
        for j in 0..15 {
            for k in 0..4 {
                let c = kb[(kr + k) & 3];
                let bit = c & 1;
                sk[j % 3] = (sk[j % 3] << 1) | bit;
                kb[(kr + k) & 3] = (c >> 1) | ((bit ^ 1) << 15);
            }
        }
    }
    ks
}

/// Decrypts `data` with 8-round ICE under `key`, block by block; bytes after the last whole
/// 8-byte block are copied unchanged.
pub(crate) fn decrypt(data: &[u8], key: &[u8; 8]) -> Vec<u8> {
    let sbox = sboxes();
    let ks = schedule(key);
    let mut out = Vec::with_capacity(data.len());
    let (blocks, tail) = data.as_chunks::<8>();
    for block in blocks {
        let [a, b, c, d, e, f, g, h] = *block;
        let mut l = u32::from_be_bytes([a, b, c, d]);
        let mut r = u32::from_be_bytes([e, f, g, h]);
        for i in (1..ROUNDS).rev().step_by(2) {
            l ^= round(r, &ks[i], sbox);
            r ^= round(l, &ks[i - 1], sbox);
        }
        out.extend_from_slice(&r.to_be_bytes());
        out.extend_from_slice(&l.to_be_bytes());
    }
    out.extend_from_slice(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The first three blocks of TF2's `scripts/tf_weapon_scattergun.ctx` from tf2_misc_dir.vpk.
    const SCATTERGUN_HEAD: [u8; 24] = [
        0x7a, 0x71, 0x07, 0x3e, 0x55, 0x89, 0x8f, 0x44, 0x04, 0x96, 0x8d, 0x75, 0x7b, 0xf5, 0x8d,
        0x77, 0x02, 0x12, 0x3f, 0x90, 0x60, 0xbf, 0x8b, 0xaf,
    ];

    #[test]
    fn decrypts_a_real_weapon_script() {
        assert_eq!(
            decrypt(&SCATTERGUN_HEAD, CTX_KEY),
            b"WeaponData\r\n{\r\n\t// Attri"
        );
    }

    #[test]
    fn a_trailing_partial_block_stays_plain() {
        let mut data = SCATTERGUN_HEAD[..16].to_vec();
        data.extend_from_slice(b"\r\n}");
        assert_eq!(decrypt(&data, CTX_KEY), b"WeaponData\r\n{\r\n\t\r\n}");
    }
}
