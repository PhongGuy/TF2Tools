//! WAV header inspection: finds the `fmt ` chunk and checks it is the one format
//! TF2 plays (16-bit 44,100 Hz stereo PCM).

use std::fmt;

use crate::error::WavError;

const PCM: u16 = 1;
const FMT_LEN: usize = 16;

/// The `fmt ` chunk of a WAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct WavInfo {
    format_tag: u16,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

impl WavInfo {
    /// The format tag; 1 is PCM.
    pub fn format_tag(&self) -> u16 {
        self.format_tag
    }

    /// The number of channels.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Samples per second.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Bits per sample.
    pub fn bits_per_sample(&self) -> u16 {
        self.bits_per_sample
    }
}

impl fmt::Display for WavInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "format {}, {} channel(s), {} Hz, {}-bit",
            self.format_tag, self.channels, self.sample_rate, self.bits_per_sample
        )
    }
}

/// Parses a WAV header; Ok only for what TF2 plays: PCM, 2 channels, 44,100 Hz, 16-bit.
///
/// Returns `Malformed` for anything that is not RIFF/WAVE with a complete `fmt `
/// chunk, and `Unsupported` (carrying the parsed format) for any other format.
pub fn inspect_wav(bytes: &[u8]) -> Result<WavInfo, WavError> {
    let info = parse_fmt(bytes)?;
    let playable = info.format_tag == PCM
        && info.channels == 2
        && info.sample_rate == 44_100
        && info.bits_per_sample == 16;
    if playable {
        Ok(info)
    } else {
        Err(WavError::Unsupported { found: info })
    }
}

/// Walks the RIFF chunks after the 12-byte header until `fmt `, skipping the
/// others with their pad byte; any length that runs past the end is malformed.
fn parse_fmt(bytes: &[u8]) -> Result<WavInfo, WavError> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(malformed("missing the RIFF/WAVE header"));
    }
    let mut pos = 12usize;
    loop {
        let header = pos
            .checked_add(8)
            .and_then(|end| bytes.get(pos..end))
            .ok_or_else(|| malformed("no fmt chunk"))?;
        let size = usize::try_from(le_u32(&header[4..8]))
            .map_err(|_| malformed("chunk size does not fit in memory"))?;
        let body = pos + 8;
        if &header[..4] == b"fmt " {
            if size < FMT_LEN {
                return Err(malformed("fmt chunk shorter than 16 bytes"));
            }
            let fmt = bytes
                .get(body..body + FMT_LEN)
                .ok_or_else(|| malformed("fmt chunk cut short"))?;
            return Ok(WavInfo {
                format_tag: le_u16(&fmt[0..2]),
                channels: le_u16(&fmt[2..4]),
                sample_rate: le_u32(&fmt[4..8]),
                bits_per_sample: le_u16(&fmt[14..16]),
            });
        }
        pos = size
            .checked_add(size & 1)
            .and_then(|padded| body.checked_add(padded))
            .ok_or_else(|| malformed("chunk size overflows"))?;
    }
}

fn malformed(message: &str) -> WavError {
    WavError::Malformed {
        message: message.to_owned(),
    }
}

/// Reads a little-endian u16 from a slice the caller sized to 2 bytes.
fn le_u16(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

/// Reads a little-endian u32 from a slice the caller sized to 4 bytes.
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
