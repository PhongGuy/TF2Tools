use std::io::{self, Read};

/// Fixed-size little-endian records read straight from the pak, in field order.
pub(crate) trait ReadLe: Sized {
    fn read_le(reader: &mut impl Read) -> io::Result<Self>;
}

pub(crate) fn bytes<const N: usize>(reader: &mut impl Read) -> io::Result<[u8; N]> {
    let mut buf = [0u8; N];
    reader.read_exact(&mut buf)?;
    Ok(buf)
}

pub(crate) fn u16_le(reader: &mut impl Read) -> io::Result<u16> {
    bytes(reader).map(u16::from_le_bytes)
}

pub(crate) fn u32_le(reader: &mut impl Read) -> io::Result<u32> {
    bytes(reader).map(u32::from_le_bytes)
}

fn u128_le(reader: &mut impl Read) -> io::Result<u128> {
    bytes(reader).map(u128::from_le_bytes)
}

#[derive(Debug, Copy, Clone)]
pub struct VPKHeader {
    pub signature: u32,
    pub version: u32,
    pub tree_length: u32,
}

impl ReadLe for VPKHeader {
    fn read_le(reader: &mut impl Read) -> io::Result<Self> {
        Ok(Self {
            signature: u32_le(reader)?,
            version: u32_le(reader)?,
            tree_length: u32_le(reader)?,
        })
    }
}

#[derive(Debug)]
pub struct VPKHeaderV2 {
    pub embed_chunk_length: u32,
    pub chunk_hashes_length: u32,
    pub self_hashes_length: u32,
    pub signature_length: u32,
}

impl ReadLe for VPKHeaderV2 {
    fn read_le(reader: &mut impl Read) -> io::Result<Self> {
        Ok(Self {
            embed_chunk_length: u32_le(reader)?,
            chunk_hashes_length: u32_le(reader)?,
            self_hashes_length: u32_le(reader)?,
            signature_length: u32_le(reader)?,
        })
    }
}

#[derive(Debug)]
pub struct VPKHeaderV2Checksum {
    pub tree_checksum: u128,
    pub chunk_hashes_checksum: u128,
    pub file_checksum: u128,
}

impl ReadLe for VPKHeaderV2Checksum {
    fn read_le(reader: &mut impl Read) -> io::Result<Self> {
        Ok(Self {
            tree_checksum: u128_le(reader)?,
            chunk_hashes_checksum: u128_le(reader)?,
            file_checksum: u128_le(reader)?,
        })
    }
}
