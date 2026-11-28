//! A bounds-checked little-endian reader over one studio file's bytes.

use crate::error::RigLoadError;

/// Longest string a studio file may name, NUL included.
const MAX_CSTR: usize = 512;

/// One file's bytes and its name, which every error carries.
pub(crate) struct Bytes<'a> {
    file: &'a str,
    data: &'a [u8],
}

impl<'a> Bytes<'a> {
    pub(crate) fn new(file: &'a str, data: &'a [u8]) -> Bytes<'a> {
        Bytes { file, data }
    }

    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    pub(crate) fn file(&self) -> &'a str {
        self.file
    }

    pub(crate) fn malformed(&self, what: impl Into<String>) -> RigLoadError {
        RigLoadError::Malformed {
            file: self.file.to_owned(),
            what: what.into(),
        }
    }

    pub(crate) fn too_large(&self) -> RigLoadError {
        RigLoadError::TooLarge {
            file: self.file.to_owned(),
        }
    }

    /// The `N` bytes at `at`, or `Malformed` when they run past the end.
    pub(crate) fn array<const N: usize>(&self, at: usize) -> Result<[u8; N], RigLoadError> {
        at.checked_add(N)
            .and_then(|end| self.data.get(at..end))
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| self.malformed(format!("{N} bytes at {at} run past the end")))
    }

    pub(crate) fn u8(&self, at: usize) -> Result<u8, RigLoadError> {
        self.array::<1>(at).map(|b| b[0])
    }

    pub(crate) fn u16(&self, at: usize) -> Result<u16, RigLoadError> {
        self.array(at).map(u16::from_le_bytes)
    }

    pub(crate) fn i16(&self, at: usize) -> Result<i16, RigLoadError> {
        self.array(at).map(i16::from_le_bytes)
    }

    pub(crate) fn i32(&self, at: usize) -> Result<i32, RigLoadError> {
        self.array(at).map(i32::from_le_bytes)
    }

    pub(crate) fn f32(&self, at: usize) -> Result<f32, RigLoadError> {
        self.array(at).map(f32::from_le_bytes)
    }

    pub(crate) fn vec3(&self, at: usize) -> Result<[f32; 3], RigLoadError> {
        Ok([self.f32(at)?, self.f32(at + 4)?, self.f32(at + 8)?])
    }

    pub(crate) fn quat(&self, at: usize) -> Result<[f32; 4], RigLoadError> {
        Ok([
            self.f32(at)?,
            self.f32(at + 4)?,
            self.f32(at + 8)?,
            self.f32(at + 12)?,
        ])
    }

    /// `base + rel`, which must lie within `0..=len`.
    pub(crate) fn rel(&self, base: usize, rel: i32) -> Result<usize, RigLoadError> {
        base.checked_add_signed(rel as isize)
            .filter(|&at| at <= self.data.len())
            .ok_or_else(|| self.malformed(format!("offset {rel} from {base} is outside the file")))
    }

    /// The NUL-terminated string at `base + rel`; rel 0 is "". Lossy UTF-8, NUL within 512 bytes.
    pub(crate) fn cstr(&self, base: usize, rel: i32) -> Result<String, RigLoadError> {
        if rel == 0 {
            return Ok(String::new());
        }
        let at = self.rel(base, rel)?;
        let window = &self.data[at..self.data.len().min(at + MAX_CSTR)];
        let end = window
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| self.malformed(format!("the string at {at} has no end")))?;
        Ok(String::from_utf8_lossy(&window[..end]).into_owned())
    }

    /// `count` entries of `stride` bytes at `base + rel`: negative count Malformed, over `cap`
    /// TooLarge, past the end Malformed; an empty table skips the offset check.
    pub(crate) fn table(
        &self,
        base: usize,
        rel: i32,
        count: i32,
        stride: usize,
        cap: usize,
    ) -> Result<Table, RigLoadError> {
        let count = usize::try_from(count)
            .map_err(|_| self.malformed(format!("a table has a negative count {count}")))?;
        if count > cap {
            return Err(self.too_large());
        }
        if count == 0 {
            return Ok(Table {
                start: 0,
                count,
                stride,
            });
        }
        let start = self.rel(base, rel)?;
        count
            .checked_mul(stride)
            .and_then(|bytes| start.checked_add(bytes))
            .filter(|&end| end <= self.data.len())
            .ok_or_else(|| {
                self.malformed(format!("a table of {count} at {start} runs past the end"))
            })?;
        Ok(Table {
            start,
            count,
            stride,
        })
    }
}

/// Where a run of fixed-size entries sits in a file, already checked to fit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Table {
    pub(crate) start: usize,
    pub(crate) count: usize,
    pub(crate) stride: usize,
}

impl Table {
    /// Where entry `i` starts.
    pub(crate) fn at(&self, i: usize) -> usize {
        self.start + i * self.stride
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_view::studio::fixture::Blob;

    fn data() -> Vec<u8> {
        let mut b = Blob::new();
        b.zeros(64);
        b.set_i32(0, -7);
        b.set_f32s(4, &[1.5]);
        b.0[8..12].copy_from_slice(b"bip\0");
        b.0
    }

    #[test]
    fn scalars_and_strings_read_little_endian() {
        let d = data();
        let b = Bytes::new("a.mdl", &d);
        assert_eq!(b.i32(0).unwrap(), -7);
        assert_eq!(b.f32(4).unwrap(), 1.5);
        assert_eq!(b.cstr(4, 4).unwrap(), "bip");
        assert_eq!(b.cstr(8, 0).unwrap(), "", "offset 0 names nothing");
        assert_eq!(b.rel(8, -8).unwrap(), 0);
    }

    #[test]
    fn reads_past_the_end_are_malformed_and_name_the_file() {
        let d = data();
        let b = Bytes::new("a.mdl", &d);
        assert!(
            matches!(b.i32(61), Err(RigLoadError::Malformed { ref file, .. }) if file == "a.mdl")
        );
        assert!(matches!(b.rel(8, 100), Err(RigLoadError::Malformed { .. })));
        assert!(matches!(b.rel(8, -9), Err(RigLoadError::Malformed { .. })));
        let unterminated = vec![b'x'; 600];
        let u = Bytes::new("a.mdl", &unterminated);
        assert!(matches!(u.cstr(0, 1), Err(RigLoadError::Malformed { .. })));
    }

    #[test]
    fn tables_check_count_cap_and_end() {
        let d = data();
        let b = Bytes::new("a.mdl", &d);
        let t = b.table(8, 8, 3, 16, 10).unwrap();
        assert_eq!((t.start, t.count, t.at(2)), (16, 3, 48));
        assert_eq!(
            b.table(0, 9999, 0, 16, 10).unwrap().count,
            0,
            "an empty table needs no offset"
        );
        assert!(matches!(
            b.table(0, 0, -1, 16, 10),
            Err(RigLoadError::Malformed { .. })
        ));
        assert!(matches!(
            b.table(0, 0, 11, 1, 10),
            Err(RigLoadError::TooLarge { .. })
        ));
        assert!(matches!(
            b.table(0, 16, 4, 16, 10),
            Err(RigLoadError::Malformed { .. })
        ));
    }
}
