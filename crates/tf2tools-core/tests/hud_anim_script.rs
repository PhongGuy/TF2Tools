//! Reading hudanimations scripts as the engine does.

use tf2tools_core::{AnimCommand, AnimScript, Interp, Platform};

fn parse(text: &str) -> AnimScript {
    AnimScript::parse(text, Platform::Linux)
}

#[test]
fn every_command_and_curve_reads() {
    let s = parse(
        r#"event Big
{
    Animate Value FgColor "TanLight" Linear 0.0 0.1
    animate Value Alpha "0" Pulse 3 0.1 0.2 // a comment
    Animate Value xpos "c-20" Bias 0.8 0 1
    RunEvent Loop 0.25
    StopEvent Loop 0
    RunEventChild Child Inner 0.5
    SetVisible Value 0 0.1
    SetInputEnabled Value 1 0
    FireCommand 0.2 "engine echo hi"
    PlaySound 0 "ui/hit.wav"
    StopAnimation Value Alpha 0
    StopPanelAnimations Value 0
    SetFont Value Font "Big" 0
}"#,
    );
    assert_eq!(s.stopped(), None);
    let c = s.events()[0].commands();
    assert_eq!(c.len(), 13);
    assert_eq!(
        c[0],
        AnimCommand::Animate {
            panel: "Value".into(),
            field: "FgColor".into(),
            value: "TanLight".into(),
            interp: Interp::Linear,
            start: 0.0,
            duration: 0.1
        }
    );
    assert!(
        matches!(&c[1], AnimCommand::Animate { interp: Interp::Pulse(p), start, .. } if *p == 3.0 && *start == 0.1)
    );
    assert!(matches!(&c[2], AnimCommand::Animate { interp: Interp::Bias(p), .. } if *p == 0.8));
    assert_eq!(
        c[3],
        AnimCommand::RunEvent {
            event: "Loop".into(),
            delay: 0.25
        }
    );
    assert_eq!(
        c[6],
        AnimCommand::SetVisible {
            panel: "Value".into(),
            visible: false,
            delay: 0.1
        }
    );
    assert_eq!(
        c[8],
        AnimCommand::FireCommand {
            delay: 0.2,
            command: "engine echo hi".into()
        }
    );
    assert_eq!(c[0].panel(), Some("Value"));
    assert_eq!(c[3].panel(), None);
}

#[test]
fn an_unknown_curve_is_linear_and_consumed() {
    let s = parse(r#"event A { Animate P Alpha "0" Wobbly 0.5 1 }"#);
    assert!(matches!(
        &s.events()[0].commands()[0],
        AnimCommand::Animate { interp: Interp::Linear, start, duration, .. } if *start == 0.5 && *duration == 1.0
    ));
}

#[test]
fn an_unknown_command_keeps_what_came_before_and_drops_the_rest() {
    let s = parse(
        r#"event A
{
    Animate P Alpha "0" Linear 0 1
    Wiggle P
    Animate P Alpha "9" Linear 0 1
}
event B { RunEvent A 0 }"#,
    );
    assert_eq!(s.stopped(), Some("Wiggle"));
    assert_eq!(s.events().len(), 1);
    assert_eq!(s.events()[0].commands().len(), 1);
}

#[test]
fn the_first_event_of_a_name_wins_ignoring_case() {
    let s = parse(r#"event Pulse { RunEvent X 0 } event PULSE { RunEvent Y 0 }"#);
    assert_eq!(s.events().len(), 1);
    assert_eq!(
        s.events()[0].commands()[0],
        AnimCommand::RunEvent {
            event: "X".into(),
            delay: 0.0
        }
    );
}

#[test]
fn conditionals_follow_the_platform() {
    let s = parse(
        r#"event A
{
    RunEvent PC 0 [$WIN32]
    RunEvent Console 0 [$X360]
}
event OnlyConsole [$X360] { RunEvent Z 0 }
event Glued{RunEvent G 0}"#,
    );
    let names: Vec<&str> = s.events().iter().map(|e| e.name()).collect();
    assert_eq!(names, ["A", "Glued"]);
    assert_eq!(
        s.events()[0].commands(),
        [AnimCommand::RunEvent {
            event: "PC".into(),
            delay: 0.0
        }]
    );
}

#[test]
fn a_body_round_trips_through_the_parser() {
    let s = parse(
        r#"event A
{
    Animate Value FgColor "255 0 0 255" Gain 0.3 0 0.00001
    FireCommand 0 "engine echo a b"
    SetVisible Value 1 0.5
}"#,
    );
    let body = s.events()[0].body();
    assert_eq!(
        body,
        "Animate \"Value\" \"FgColor\" \"255 0 0 255\" Gain 0.3 0 0.00001\nFireCommand 0 \"engine echo a b\"\nSetVisible \"Value\" 1 0.5"
    );
    let again = parse(&format!("event A\n{{\n{body}\n}}\n"));
    assert_eq!(again.events()[0].commands(), s.events()[0].commands());
}

#[test]
fn bad_times_skip_their_line_and_count() {
    let s = parse(
        r#"event A
{
    Animate P Alpha "0" Linear nan 1
    Animate P Alpha "0" Linear 0 inf
    RunEvent B -0.5
    RunEvent C 0
}"#,
    );
    assert_eq!(
        s.events()[0].commands(),
        [AnimCommand::RunEvent {
            event: "C".into(),
            delay: 0.0
        }]
    );
    assert_eq!(s.skipped(), 3);
}

#[test]
fn caps_skip_and_count() {
    let long = "x".repeat(513);
    let s = parse(&format!("event A {{ RunEvent {long} 0\nRunEvent Ok 0 }}"));
    assert_eq!(s.events()[0].commands().len(), 1);
    assert_eq!(s.skipped(), 1);
    let many = "RunEvent X 0\n".repeat(257);
    let s = parse(&format!("event A {{\n{many}}}"));
    assert_eq!((s.events()[0].commands().len(), s.skipped()), (256, 1));
    let events: String = (0..4097).map(|i| format!("event E{i} {{ }}\n")).collect();
    let s = parse(&events);
    assert_eq!((s.events().len(), s.skipped()), (4096, 1));
}

#[test]
fn a_token_holding_a_line_break_skips_its_command() {
    let s = parse(
        "event A\n{\n    SetVisible \"x 1 0\nFireCommand 0 quit\nSetVisible y\" 1 0\n    RunEvent B 0\n}",
    );
    assert_eq!(
        s.events()[0].commands(),
        [AnimCommand::RunEvent {
            event: "B".into(),
            delay: 0.0
        }]
    );
    assert_eq!(s.skipped(), 1);
}

#[test]
fn a_token_that_needs_quotes_is_written_quoted() {
    let s = parse(r#"event A { RunEvent "B C" 0 SetFont "P Q" Font "Big One" 0 }"#);
    let body = s.events()[0].body();
    assert_eq!(
        body,
        "RunEvent \"B C\" 0\nSetFont \"P Q\" \"Font\" \"Big One\" 0"
    );
    let again = parse(&format!("event A\n{{\n{body}\n}}\n"));
    assert_eq!(again.events()[0].commands(), s.events()[0].commands());
}
