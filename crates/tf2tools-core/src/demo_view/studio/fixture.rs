//! Byte builders for synthetic studio files, for the parser tests.

/// Bytes that grow as a file is laid out, with in-place patches of earlier fields.
pub(crate) struct Blob(pub(crate) Vec<u8>);

impl Blob {
    pub(crate) fn new() -> Blob {
        Blob(Vec::new())
    }
    /// Appends `bytes`; returns where they start.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> usize {
        let at = self.0.len();
        self.0.extend_from_slice(bytes);
        at
    }
    pub(crate) fn zeros(&mut self, n: usize) -> usize {
        self.push(&vec![0; n])
    }
    pub(crate) fn cstr(&mut self, s: &str) -> usize {
        let at = self.push(s.as_bytes());
        self.0.push(0);
        at
    }
    pub(crate) fn set_i32(&mut self, at: usize, v: i32) {
        self.0[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn set_i16(&mut self, at: usize, v: i16) {
        self.0[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub(crate) fn set_f32s(&mut self, at: usize, v: &[f32]) {
        for (i, x) in v.iter().enumerate() {
            self.0[at + 4 * i..at + 4 * i + 4].copy_from_slice(&x.to_le_bytes());
        }
    }
    /// Sets the i32 at `field` to `target - base`.
    pub(crate) fn set_rel(&mut self, field: usize, base: usize, target: usize) {
        self.set_i32(field, target as i32 - base as i32);
    }
}
