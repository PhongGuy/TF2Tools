//! The HL2DEMO header: map name and length from the first 1072 bytes, and the refusals.

use crate::common::demo_view::demo_header;
use tf2tools_core::{DemoHeaderError, read_demo_header};

#[test]
fn the_header_names_the_map_and_the_length() {
    let header = read_demo_header(&demo_header("cp_process_final", 1800.5)).unwrap();
    assert_eq!(header.map(), "cp_process_final");
    assert_eq!(header.duration_seconds(), 1800.5);
}

#[test]
fn packets_after_the_header_are_never_read() {
    let mut dem = demo_header("koth_product_final", 60.0);
    dem.extend_from_slice(&[0xFF; 4096]);
    assert_eq!(read_demo_header(&dem).unwrap().map(), "koth_product_final");
}

#[test]
fn wrong_magic_is_not_a_demo() {
    let mut dem = demo_header("cp_process_final", 1.0);
    dem[0] = b'X';
    assert!(matches!(
        read_demo_header(&dem),
        Err(DemoHeaderError::NotADemo)
    ));
    assert!(matches!(
        read_demo_header(b"hello"),
        Err(DemoHeaderError::NotADemo)
    ));
}

#[test]
fn a_short_file_is_truncated() {
    let dem = demo_header("cp_process_final", 1.0);
    for len in [0, 4, 8, 600, 1071] {
        assert!(
            matches!(
                read_demo_header(&dem[..len]),
                Err(DemoHeaderError::Truncated)
            ),
            "{len} bytes"
        );
    }
}

#[test]
fn a_map_name_that_is_not_a_plain_file_name_is_refused_with_its_name() {
    for map in ["../x", "workshop/x.ugc1", "a b", ""] {
        match read_demo_header(&demo_header(map, 1.0)) {
            Err(DemoHeaderError::BadMapName { name }) => assert_eq!(name, map),
            other => panic!("{map:?}: {other:?}"),
        }
    }
}
