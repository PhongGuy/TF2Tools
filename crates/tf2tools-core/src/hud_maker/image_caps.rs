//! The limits on pictures a made HUD borrows from other HUDs, applied on save and on reopen.

/// The largest single borrowed file.
pub(crate) const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// The most borrowed files in one HUD; a material and its texture each count.
pub(crate) const MAX_IMAGE_FILES: usize = 64;
/// The most bytes of borrowed files in one HUD.
pub(crate) const MAX_IMAGE_TOTAL: u64 = 256 * 1024 * 1024;

/// Whether files of these `sizes` fit all three caps.
pub(crate) fn within_caps(sizes: &[u64]) -> bool {
    sizes.len() <= MAX_IMAGE_FILES
        && sizes.iter().all(|&s| s <= MAX_IMAGE_BYTES)
        && sizes
            .iter()
            .try_fold(0u64, |t, &s| t.checked_add(s))
            .is_some_and(|t| t <= MAX_IMAGE_TOTAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_count_the_file_size_and_the_total_trip_exactly() {
        assert!(within_caps(&vec![1; MAX_IMAGE_FILES]));
        assert!(!within_caps(&vec![1; MAX_IMAGE_FILES + 1]));
        assert!(
            within_caps(&[MAX_IMAGE_BYTES; 16]),
            "16 x 16 MiB is exactly 256 MiB"
        );
        assert!(!within_caps(&[vec![MAX_IMAGE_BYTES; 16], vec![1]].concat()));
        assert!(!within_caps(&[MAX_IMAGE_BYTES + 1]));
    }
}
