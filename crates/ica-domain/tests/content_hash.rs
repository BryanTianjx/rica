//! The message fingerprint and the enumerations the wire format exposes.
//!
//! `content_hash` must be byte-identical to Python's
//! `sha256(" ".join(content.split()).lower())`: the duplicate detector compares
//! stored hashes, so a different normalisation would silently stop matching
//! messages imported on the other side.

use ica_domain::{
    content_hash, CandidateStatus, MessageSource, PositionStatus, ResultType, SenderType,
};

#[test]
fn the_fingerprint_matches_the_measured_python_value() {
    // Captured from the Python baseline: sha256("msg 0").
    assert_eq!(
        content_hash("msg 0"),
        "bf40f3ea98f97bcfdc40cec1c291497e2dd089d05653bd389189c57b6ece0aed"
    );
}

#[test]
fn whitespace_runs_collapse_and_case_folds() {
    // sha256("hello world")
    assert_eq!(
        content_hash("  Hello   World  "),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
    );
    assert_eq!(
        content_hash("hello world"),
        content_hash("  Hello \t World ")
    );
}

#[test]
fn an_empty_body_hashes_the_empty_string() {
    // Python: " ".join("".split()) == "", so the digest is sha256("").
    assert_eq!(
        content_hash(""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(content_hash("   \n\t "), content_hash(""));
}

#[test]
fn different_bodies_hash_differently() {
    assert_ne!(content_hash("msg 0"), content_hash("msg 1"));
}

#[test]
fn multibyte_content_is_hashed_as_utf8() {
    // The normalisation is applied before hashing, so the same body with
    // different spacing shares one fingerprint.
    assert_eq!(content_hash("你好   世界"), content_hash(" 你好 世界 "));
    assert_ne!(content_hash("你好 世界"), content_hash("你好 世界!"));
}

#[test]
fn position_statuses_round_trip() {
    for status in [
        PositionStatus::Draft,
        PositionStatus::Open,
        PositionStatus::OnHold,
        PositionStatus::Closed,
    ] {
        assert_eq!(PositionStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(
        PositionStatus::parse("open"),
        None,
        "values are case-sensitive"
    );
    assert!(PositionStatus::Closed.blocks_follow_up());
    assert!(!PositionStatus::Open.blocks_follow_up());
}

#[test]
fn candidate_statuses_round_trip() {
    for status in [
        CandidateStatus::New,
        CandidateStatus::Contacted,
        CandidateStatus::ReadNoReply,
        CandidateStatus::Replied,
        CandidateStatus::Active,
        CandidateStatus::Interview,
        CandidateStatus::Rejected,
        CandidateStatus::Withdrawn,
        CandidateStatus::Closed,
    ] {
        assert_eq!(CandidateStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(CandidateStatus::parse("new"), None);
    assert!(CandidateStatus::Rejected.is_terminal());
    assert!(!CandidateStatus::Interview.is_terminal());
}

#[test]
fn message_enumerations_round_trip() {
    for sender in [
        SenderType::Candidate,
        SenderType::Recruiter,
        SenderType::System,
    ] {
        assert_eq!(SenderType::parse(sender.as_str()), Some(sender));
    }
    for source in [
        MessageSource::Manual,
        MessageSource::PasteImport,
        MessageSource::ScreenshotOcr,
    ] {
        assert_eq!(MessageSource::parse(source.as_str()), Some(source));
    }
    // V1.0 has no browser-capture source; the value must not be accepted.
    assert_eq!(MessageSource::parse("browser_capture"), None);
}

#[test]
fn result_types_round_trip_and_only_contact_outcomes_count() {
    for outcome in [
        ResultType::Sent,
        ResultType::NotSent,
        ResultType::CandidateReplied,
        ResultType::CandidateNoReply,
        ResultType::CandidateDeclined,
        ResultType::Interview,
        ResultType::Other,
    ] {
        assert_eq!(ResultType::parse(outcome.as_str()), Some(outcome));
    }
    assert!(ResultType::Sent.indicates_contact());
    assert!(ResultType::CandidateReplied.indicates_contact());
    assert!(ResultType::CandidateDeclined.indicates_contact());
    assert!(ResultType::Interview.indicates_contact());
    assert!(!ResultType::NotSent.indicates_contact());
    assert!(!ResultType::CandidateNoReply.indicates_contact());
    assert!(!ResultType::Other.indicates_contact());
}
