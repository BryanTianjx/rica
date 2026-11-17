//! ``PositionStatus`` must round-trip the exact strings the Python baseline
//! stores: ``app/domain/enums.py::PositionStatus`` uses capitalised values,
//! not lowercase slugs. A mismatch here would corrupt the API contract.

use ica_domain::PositionStatus;

#[test]
fn parses_every_value_the_python_enum_defines() {
    let cases = [
        ("Draft", PositionStatus::Draft),
        ("Open", PositionStatus::Open),
        ("OnHold", PositionStatus::OnHold),
        ("Closed", PositionStatus::Closed),
    ];

    for (raw, expected) in cases {
        assert_eq!(PositionStatus::parse(raw), Some(expected), "raw={raw}");
    }
}

#[test]
fn rejects_lowercase_and_slug_forms() {
    // The Python enum only accepts the exact `.value` strings; a lowercase
    // slug in the column would be a data defect, not a valid status.
    assert_eq!(PositionStatus::parse("open"), None);
    assert_eq!(PositionStatus::parse("on_hold"), None);
    assert_eq!(PositionStatus::parse("On Hold"), None);
}

#[test]
fn rejects_empty_and_unknown_values() {
    assert_eq!(PositionStatus::parse(""), None);
    assert_eq!(PositionStatus::parse("Paused"), None);
}

#[test]
fn renders_the_same_string_it_parsed() {
    for status in [
        PositionStatus::Draft,
        PositionStatus::Open,
        PositionStatus::OnHold,
        PositionStatus::Closed,
    ] {
        assert_eq!(PositionStatus::parse(status.as_str()), Some(status));
    }
}

#[test]
fn only_closed_blocks_follow_up() {
    assert!(!PositionStatus::Draft.blocks_follow_up());
    assert!(!PositionStatus::Open.blocks_follow_up());
    assert!(!PositionStatus::OnHold.blocks_follow_up());
    assert!(PositionStatus::Closed.blocks_follow_up());
}
