//! The engine's position and size expressions, evaluated in pixels at 1920x1080.

use tf2tools_core::{Align, AxisSize, PosExpr, PosUnit, ScreenScale, SizeExpr, SizeForm, TermOp};

const HD: u32 = 1080;

fn pos(text: &str, own: i32, parent: i32) -> i32 {
    PosExpr::parse(text).eval(own, parent, ScreenScale::new(HD))
}

fn size(text: &str, current: i32, parent: i32) -> i32 {
    let none = AxisSize::new(0, 0);
    SizeExpr::parse(text).eval(
        AxisSize::new(current, parent),
        None,
        none,
        ScreenScale::new(HD),
    )
}

#[test]
fn proportional_units_scale_by_screen_height_and_truncate() {
    assert_eq!(ScreenScale::new(1080).scale(10), 22);
    assert_eq!(ScreenScale::new(1080).scale(-7), -15);
    assert_eq!(ScreenScale::new(1200).scale(10), 25);
    assert_eq!(ScreenScale::new(1024).scale(15), 32);
    assert_eq!(ScreenScale::new(1080).normalise(45), 20);
}

#[test]
fn a_zero_screen_height_scales_as_one_pixel() {
    let zero = ScreenScale::new(0);
    assert_eq!(zero, ScreenScale::new(1));
    assert_eq!((zero.scale(480), zero.normalise(1)), (1, 480));
}

#[test]
fn positions_follow_compute_pos() {
    let cases: &[(&str, i32, i32, i32)] = &[
        ("10", 0, 1920, 22),
        ("-7", 0, 1920, -15),
        ("c-100", 0, 1920, 735),
        ("r120", 0, 1080, 810),
        ("rs1-10", 100, 1920, 1798),
        ("cs-0.5", 200, 1920, 860),
        ("p0.25", 0, 1920, 480),
        ("rp0.5+s-1-40", 100, 1920, 770),
        ("c17+20", 0, 1920, 1043),
        ("2++130+140+140+90", 0, 1920, 1128),
        ("c", 0, 1920, 960),
        ("-p0.01", 0, 1920, 0),
        ("r98`", 0, 1920, 1700),
        ("4w", 0, 1920, 9),
    ];
    for &(text, own, parent, want) in cases {
        assert_eq!(pos(text, own, parent), want, "{text}");
    }
}

#[test]
fn positions_report_whether_they_fit_the_grammar() {
    for text in [
        "10",
        "-7",
        "c-100",
        "r120",
        "rs1-10",
        "cs-0.5",
        "p0.25",
        "rp0.5+s-1-40",
        "c",
        "cs",
        "c17+20",
        "0.5",
        ".5",
    ] {
        assert!(PosExpr::parse(text).is_well_formed(), "{text}");
    }
    for text in ["", "-p0.01", "r98`", "4w", "f0", " 10", "c-", "1e2"] {
        assert!(!PosExpr::parse(text).is_well_formed(), "{text}");
    }
}

#[test]
fn position_terms_keep_their_form() {
    let expr = PosExpr::parse("rs1-10");
    assert_eq!(expr.text(), "rs1-10");
    let t = expr.terms();
    assert_eq!(t.len(), 2);
    assert_eq!(
        (t[0].op(), t[0].align(), t[0].unit(), t[0].number()),
        (TermOp::Set, Align::End, PosUnit::OwnSize, "1")
    );
    assert_eq!(
        (t[1].op(), t[1].align(), t[1].unit(), t[1].number()),
        (TermOp::Sub, Align::Start, PosUnit::Units, "10")
    );
    let c = PosExpr::parse("c-100");
    assert_eq!(
        (c.terms()[0].align(), c.terms()[0].number()),
        (Align::Centre, "-100")
    );
    assert_eq!(
        PosExpr::parse("p0.25").terms()[0].unit(),
        PosUnit::ParentSize
    );
}

#[test]
fn sizes_follow_compute_wide_and_tall() {
    let cases: &[(&str, i32, i32, i32)] = &[
        ("20", 0, 1920, 45),
        ("f0", 0, 1920, 1920),
        ("f", 0, 1920, 1920),
        ("f+50", 0, 1920, 1808),
        ("f0-40", 0, 1920, 1920),
        ("p0.5", 0, 1920, 960),
        ("p1-200", 0, 1920, 1918),
        ("s2", 100, 1920, 200),
        ("250`", 0, 1920, 562),
        ("w1", 0, 1920, 0),
        ("47d", 0, 1920, 105),
        ("124-p0.001", 0, 1920, 279),
    ];
    for &(text, current, parent, want) in cases {
        assert_eq!(size(text, current, parent), want, "{text}");
    }
}

#[test]
fn o_sizes_follow_the_other_axis() {
    let scale = ScreenScale::new(HD);
    let tall = SizeExpr::parse("20");
    let wide = |text: &str| {
        SizeExpr::parse(text).eval(
            AxisSize::new(0, 1920),
            Some(&tall),
            AxisSize::new(0, 1080),
            scale,
        )
    };
    assert_eq!(wide("o1"), 45);
    assert_eq!(wide("o0.5"), 22);
    let other_o = SizeExpr::parse("o1");
    let both = SizeExpr::parse("o1").eval(
        AxisSize::new(0, 1920),
        Some(&other_o),
        AxisSize::new(0, 1080),
        scale,
    );
    assert_eq!(both, 0, "wide and tall both o is zero");
    let absent =
        SizeExpr::parse("o2").eval(AxisSize::new(0, 1920), None, AxisSize::new(45, 1080), scale);
    assert_eq!(
        absent, 90,
        "an unset other key uses the other axis's current size"
    );
}

#[test]
fn sizes_report_their_form_and_grammar() {
    let forms = [
        ("20", SizeForm::Units),
        ("f0", SizeForm::Full),
        ("p0.5", SizeForm::ParentFrac),
        ("s2", SizeForm::OwnFrac),
        ("o1", SizeForm::OtherAxis),
    ];
    for (text, form) in forms {
        let expr = SizeExpr::parse(text);
        assert_eq!(expr.form(), form, "{text}");
        assert!(expr.is_well_formed(), "{text}");
    }
    assert_eq!(SizeExpr::parse("f0").number(), "0");
    for text in ["f", "f+50", "-5"] {
        assert!(SizeExpr::parse(text).is_well_formed(), "{text}");
    }
    for text in ["", "f0-40", "p1-200", "250`", "w1", "47d", "r2"] {
        assert!(!SizeExpr::parse(text).is_well_formed(), "{text}");
    }
}

#[test]
fn numbers_read_as_c_atoi_and_atof() {
    assert_eq!(pos(" 10", 0, 1920), 22, "atoi skips leading space");
    assert_eq!(pos("s1e-1", 100, 1920), 10, "atof reads an exponent");
    assert_eq!(pos("s1e", 100, 1920), 100, "a bare e is not an exponent");
    assert_eq!(pos("+", 0, 1920), 0, "a lone sign reads as 0");
    assert_eq!(
        size("99999999999", 0, 1920),
        i32::MAX,
        "atoi and scaling clamp to i32"
    );
}
