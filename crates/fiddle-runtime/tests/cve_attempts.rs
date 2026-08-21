use fiddle_runtime::cve::attempts::{read, write, AttemptsError};

const NO_MARKER: &str = "\
Bump the base image to clear the advisories this run selected.

Mitigated: CVE-2026-1
";

const TWO_ATTEMPTS: &str = "\
Bump the base image to clear the advisories this run selected.

<!-- fiddle-attempts:start -->
Attempts: 2
<!-- fiddle-attempts:end -->

Mitigated: CVE-2026-1
";

const A_WORD_WHERE_THE_COUNT_BELONGS: &str = "\
Bump the base image to clear the advisories this run selected.

<!-- fiddle-attempts:start -->
Attempts: a few, I reset this by hand
<!-- fiddle-attempts:end -->

Mitigated: CVE-2026-1
";

#[test]
fn a_body_with_no_marker_records_no_attempts() {
    assert_eq!(
        read(NO_MARKER).expect("a body fiddle has not yet counted in is readable"),
        0,
        "the first run over a pull request finds no block, and no block means no \
         attempt has been made yet"
    );
}

#[test]
fn a_body_with_a_marker_records_the_number_it_holds() {
    assert_eq!(
        read(TWO_ATTEMPTS).expect("a block holding a number is readable"),
        2,
        "the block is the durable record across runs, so the number in it is the count"
    );
}

#[test]
fn a_marker_holding_a_word_refuses_and_names_the_body_not_a_count() {
    let err = read(A_WORD_WHERE_THE_COUNT_BELONGS)
        .expect_err("prose where the count belongs is not a count");

    assert!(
        err.to_string().contains("body"),
        "the caller must refuse rather than guess, so the error names the body it \
         cannot read: {err}"
    );
    assert!(
        !err.to_string().contains('0'),
        "reporting zero here would let an edited body reset the bound and attempt \
         forever, so the error reports no count at all: {err}"
    );
    assert!(
        matches!(err, AttemptsError::Unreadable { .. }),
        "the body is unreadable, which is a different fact from a count of zero: {err:?}"
    );
}

#[test]
fn a_rewrite_replaces_the_block_and_leaves_every_other_line_intact() {
    let written = write(TWO_ATTEMPTS, 3).expect("a body with a readable block accepts a rewrite");

    assert_eq!(
        read(&written).expect("what write leaves behind, read reads"),
        3,
        "the rewrite carries the new count"
    );
    for line in TWO_ATTEMPTS.lines().filter(|line| *line != "Attempts: 2") {
        assert!(
            written.lines().any(|kept| kept == line),
            "fiddle owns the shared body across runs, so a rewrite of the count \
             must not drop {line:?}: {written:?}"
        );
    }
    assert!(
        !written.contains("Attempts: 2"),
        "the old count is replaced, not appended: {written:?}"
    );
}

#[test]
fn a_first_write_adds_the_block_and_keeps_the_body_it_found() {
    let written = write(NO_MARKER, 1).expect("a body with no block accepts a first write");

    assert_eq!(
        read(&written).expect("what write leaves behind, read reads"),
        1,
        "the first attempt is recorded where the next run will look"
    );
    assert!(
        written.starts_with(NO_MARKER),
        "every byte the body already held stays where it was: {written:?}"
    );
}

#[test]
fn a_count_in_the_prose_outside_the_block_is_not_the_count() {
    let body = "The last run said Attempts: 7, which is prose and not the record.\n";

    assert_eq!(
        read(body).expect("prose naming attempts carries no block"),
        0,
        "only the marker-delimited block is the record, because a human writes prose \
         and fiddle writes the block"
    );
}

#[test]
fn a_start_marker_with_no_end_marker_refuses() {
    let body = "Bump the base image.\n\n<!-- fiddle-attempts:start -->\nAttempts: 2\n";
    let err = read(body).expect_err("half a block delimits nothing");

    assert!(
        err.to_string().contains("body"),
        "a hand-truncated block is a body fiddle cannot read: {err}"
    );
}

#[test]
fn an_end_marker_with_no_start_marker_refuses() {
    let body = "Bump the base image.\n\nAttempts: 2\n<!-- fiddle-attempts:end -->\n";
    let err = read(body).expect_err("half a block delimits nothing");

    assert!(
        err.to_string().contains("body"),
        "a hand-truncated block is a body fiddle cannot read: {err}"
    );
}

#[test]
fn a_body_holding_two_blocks_refuses_rather_than_choosing_one() {
    let body = format!("{TWO_ATTEMPTS}\n{TWO_ATTEMPTS}");
    let err = read(&body).expect_err("two blocks name no single count");

    assert!(
        err.to_string().contains("body"),
        "choosing between two blocks would guess, and the caller's job is to refuse: {err}"
    );
}

#[test]
fn a_rewrite_over_a_body_it_cannot_read_refuses_rather_than_mangling_it() {
    let err = write(A_WORD_WHERE_THE_COUNT_BELONGS, 3)
        .expect_err("a block that reads as nothing is not a block to overwrite");

    assert!(
        matches!(err, AttemptsError::Unreadable { .. }),
        "read and write refuse the same bodies, so neither can repair what the \
         other refuses: {err:?}"
    );
}
