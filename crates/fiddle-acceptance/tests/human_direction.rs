//! The scripted world a decision walk needs, asserted before anything walks it.
//!
//! M3's central claim is proven by deleting things: a process suspends, every
//! local record of it is taken away, and a second process continues from the
//! conversation alone. That proof is only as good as the deletion, so the
//! deletion is what this file tests. A `delete_workspaces` that silently emptied
//! nothing would leave `fiddle-565u`'s walk passing while proving nothing at all
//! — the second process would be reading a past it was supposed to have lost.
//!
//! So every helper here is asserted against a **denominator**: there was
//! something to delete, named and counted, before the delete ran. "Found
//! nothing" and "looked at nothing" must not be the same observation.
//!
//! # What this lane can reach, and the one thing it cannot
//!
//! `run --capability propose_change` does not execute in this build.
//! `fiddle-cli/src/main.rs:827` is `Selection::Propose => Err(Unbuildable { … })`,
//! and the comment above it names what is missing — an `EffectContext` whose
//! worktree is the tree the attempt will *create*, and a `DecisionTrace` for the
//! walk to announce itself to — and says both belong to the bean that gates a
//! suspension end to end. That is `fiddle-565u`, whose own Step 3 says the same
//! and whose commit stages `main.rs`.
//!
//! Everything that does not need a suspension is therefore proven here, and the
//! one property that does is named where it is missing rather than quietly
//! dropped: see [`the_suspended_path_is_not_yet_reachable_through_the_binary`].
//! The accessors that property will need — `all_published_bytes`,
//! `the_only_request_comment` — are built and exercised regardless, so 565u
//! inherits helpers this lane has already watched work.

mod support;

use support::{Comment, World, AUTHORIZED, FIDDLE_BOT, INVOCATION_REF, STRANGER};

// ---------------------------------------------------------------------------
// `inspect` stays read-only, for this capability too
// ---------------------------------------------------------------------------

/// `inspect` stays read-only and credential-free for `propose_change` too, which
/// is the property that makes it safe to run against anything.
///
/// The three assertions are the three channels this capability would mutate if it
/// ran: a branch on the remote, a comment on the conversation, and a request to
/// the forge at all. The last is the widest — a read that reached `gh` would be
/// recorded whatever it asked for — so it is the one that would catch a future
/// `inspect` that grew a credentialled lookup rather than a write.
#[test]
fn inspect_builds_nothing_for_propose_change() {
    let world = World::new();
    let out = world.fiddle_without_credentials([
        "inspect",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);

    assert_eq!(out.code, Some(0), "stderr: {}", out.stderr);
    assert_eq!(world.remote_branches().len(), 0);
    assert!(
        world.posted_comment_bodies().is_empty(),
        "posted {:?}",
        world.posted_comment_bodies()
    );
    assert!(
        world.requested_paths().is_empty(),
        "inspect consulted the forge at {:?}",
        world.requested_paths()
    );
    assert!(
        !world.report_dir().exists(),
        "inspect published something to {}",
        world.report_dir().display()
    );
}

// ---------------------------------------------------------------------------
// The deletion helpers, each against its own denominator
// ---------------------------------------------------------------------------

/// The deletion helpers are the load-bearing part of the fixture, so they are
/// tested rather than trusted: a helper that silently deleted nothing would make
/// Task 13b's proof vacuous while it still passed.
///
/// # Two runs, because no single run leaves all three kinds of record
///
/// This was going to be one repair, and the denominators are what said it could
/// not be. A run that *finishes* leaves exactly one of the three:
///
/// - it publishes a bundle;
/// - it **removes** its own journal record. `journal.rs:236-245`'s `supersede`
///   deletes the file once the bundle has landed, on the argument that the
///   attempt is then fully recorded elsewhere — so a completed run's journal is
///   empty by design, not by accident;
/// - it takes its worktree down, which
///   `binary_repair::the_binary_drives_a_repair_that_passes_its_check_and_records_the_marker`
///   asserts directly.
///
/// So a test of the three helpers against one completed repair would have found
/// nothing to delete for two of them, deleted nothing, and passed. That is the
/// exact vacuous proof this bean exists to prevent, and it would have been
/// reached by writing the obvious test.
///
/// The history here is therefore the one an operator really accumulates: a run
/// that finished, and then a run that died. The second is killed inside its
/// worktree, so it leaves both the journal record it never got to supersede and
/// the checkout it never got to remove.
///
/// # Why `delete_report_bundles` spares `.attempts`
///
/// The journal lives *inside* the report directory, at `<report.dir>/.attempts` —
/// `Scenario::prepare_journal_dir` is where that layout is written down. So the
/// tidy implementation of `delete_report_bundles` is
/// `remove_dir_all(<report.dir>)`, which `Scenario::remove_local_records` already
/// is; and under it `delete_attempt_journal` **can never fail**, because there is
/// never anything left for it to delete. One helper would be doing the work of
/// two and the second would be untested by construction.
///
/// Hence the shape below: after each delete, the local past is asserted to be
/// *still not empty*, and that intermediate assertion is the whole design. It is
/// what says the next helper is about to run against something rather than
/// inheriting a sibling's work.
#[cfg(unix)]
#[test]
fn deleting_the_local_past_really_deletes_it() {
    let world = World::new();

    // A run that finished: the bundle.
    let finished = world.repair();
    assert_eq!(finished.code, Some(0), "stderr: {}", finished.stderr);
    assert!(
        world.worktrees().is_empty(),
        "a completed attempt takes its worktree down, so there is nothing here \
         yet for `delete_workspaces` to be about: {:?}",
        world.worktrees()
    );

    // A run that died: the journal record and the worktree.
    let leftover = world.interrupt_a_repair_inside_its_worktree();
    assert!(
        !leftover.is_empty(),
        "the killed attempt left no worktree, so there is nothing to delete"
    );

    // The three denominators, named before anything is taken away.
    let bundles = world.report_bundles();
    let records = world.journal_records();
    assert!(
        !bundles.is_empty(),
        "there must be a published bundle to delete, and {} holds none",
        world.report_dir().display()
    );
    assert!(
        !records.is_empty(),
        "there must be an attempt record to delete, and {} holds none",
        world.report_dir().join(".attempts").display()
    );
    assert!(
        !world.worktrees().is_empty(),
        "there must be a workspace to delete"
    );
    assert!(
        !world.local_state_is_empty(),
        "there must be something to delete"
    );
    // And the fourth denominator, which is about the accessor rather than about
    // the state: `all_published_bytes` must be reading the bundles it claims to.
    // An inversion made it return nothing at all and **no test noticed**, because
    // the only assertion on it was the `== ""` at the end of this test — which
    // holds just as well of an accessor that reads nothing. This line is what
    // makes that final assertion mean something.
    let published = world.all_published_bytes();
    assert!(
        published.contains(INVOCATION_REF),
        "the published bytes must be this run's, and they are {} long",
        published.len()
    );

    world.delete_report_bundles();
    assert!(
        world.report_bundles().is_empty(),
        "the bundles survived: {:?}",
        world.report_bundles()
    );
    // The assertion the union implementation cannot pass.
    assert!(
        !world.journal_records().is_empty(),
        "deleting the bundles also deleted the journal, so `delete_attempt_journal` \
         is about to pass against nothing: it held {records:?}"
    );
    assert!(
        !world.local_state_is_empty(),
        "the journal and the worktree are still there"
    );

    world.delete_attempt_journal();
    assert!(
        world.journal_records().is_empty(),
        "the journal survived: {records:?}"
    );
    // And the second of the same shape: `delete_workspaces` must still have work.
    assert!(
        !world.local_state_is_empty(),
        "deleting the journal also emptied the workspace root, so \
         `delete_workspaces` is about to pass against nothing: it held {leftover:?}"
    );

    world.delete_workspaces();
    assert!(
        world.worktrees().is_empty(),
        "the worktree survived: {leftover:?}"
    );
    assert!(
        world.local_state_is_empty(),
        "the helpers must actually empty it"
    );
    assert_eq!(
        world.all_published_bytes(),
        "",
        "nothing fiddle published may survive the deletion"
    );
}

// ---------------------------------------------------------------------------
// The conversation
// ---------------------------------------------------------------------------

/// The comment collection is mutable and ordered by id, because every candidate
/// rule in the validation order depends on that ordering being real rather than
/// an artefact of how the fixture happens to serialise.
///
/// **What a test using `conversation` still cannot distinguish:** the order the
/// *listing* returns from the order the ids imply. They are deliberately allowed
/// to disagree — the stub merges fiddle's own posts onto the last page whatever
/// their ids — and `validate::select_candidates` decides by id. So this asserts
/// the ids, and a test that wanted the listing's own order has to read it through
/// [`World::listing`].
#[test]
fn the_scripted_conversation_is_mutable_and_ordered_by_id() {
    let world = World::new();
    let first = world.post_comment(AUTHORIZED, "one");
    let second = world.post_comment(AUTHORIZED, "two");
    assert!(second > first, "{first} then {second}");

    world.edit_comment(first, "one, edited");

    let all = world.conversation();
    assert_eq!(
        all.iter().map(|c| c.id).collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(all[0].body, "one, edited");
    // The stamps by **value** and not only by their relation to each other.
    //
    // `updated_at` is discriminating: `EDITED_AT` differs from `SEEDED_AT`, so a
    // `comment_from` that hardcoded the field fails here.
    //
    // `created_at` is **not**, and it is worth saying rather than implying
    // otherwise. Hardcoding it to `SEEDED_AT` in `comment_from` was inverted and
    // **no test noticed** — because `SEEDED_AT` is the only value anything in this
    // world ever writes to it, so reading the field and inventing it are
    // indistinguishable by construction. Closing that would mean adding a way to
    // seed a comment with some other creation stamp, which nothing needs: the
    // property the walk turns on is the *relation* — equal stamps mean nobody has
    // edited it — and that half is protected, because moving `updated_at` is what
    // `edit_comment` does and breaking it fails this test.
    assert_eq!(all[0].created_at, support::SEEDED_AT);
    assert_eq!(all[0].updated_at, support::EDITED_AT);
    assert_ne!(all[0].updated_at, all[0].created_at);
    // The comment nobody touched keeps its stamps equal, which is what makes the
    // assertion above about the *edit* rather than about how the fixture writes
    // timestamps in general.
    assert_eq!(all[1].created_at, support::SEEDED_AT);
    assert_eq!(all[1].updated_at, support::SEEDED_AT);
    assert_eq!(all[1].updated_at, all[1].created_at);
}

/// A question posted **through the forge** appears on the conversation, beside the
/// comment that was already there.
///
/// # The third instance of the sharpened rule, and the one 565u rests on
///
/// The stub merges the comments a run posted onto the listing, **keyed on the exact
/// path**. Nothing in this lane asserted the merge: `post_comment_through_the_forge`
/// was only ever followed by `posted_comment_bodies`, which reads the *request log*.
/// So `CONVERSATION_ISSUE`'s value appeared only in positions where any value would
/// have done — the sharpened rule exactly, and not an accessor asserted empty.
///
/// It is also the mechanism `fiddle-565u` depends on most directly: a suspended run
/// publishes its question by posting it, and `the_only_request_comment` finds it
/// only if the merge puts it on the listing. Every previous exercise of that
/// accessor used [`World::seed_question`], which writes a page file — so the
/// accessor was proven against a *constructed* question and never against a posted
/// one. This is the posted one.
#[test]
fn a_question_posted_through_the_forge_appears_on_the_conversation() {
    let world = World::new();
    let earlier = world.post_comment(AUTHORIZED, "a person got there first");

    let answer = world.post_comment_through_the_forge("May fiddle mark it ready for review?");
    assert!(
        answer.contains("HTTP/2.0 201"),
        "the write must have been accepted, or there is nothing to merge: {answer}"
    );

    let only = world.the_only_request_comment();
    assert_eq!(only.body, "May fiddle mark it ready for review?");
    assert_eq!(only.author, FIDDLE_BOT);
    assert!(only.is_bot);
    // Above the comment that preceded it, which is the ordering every candidate
    // rule in the validation order decides by.
    assert!(
        only.id > earlier,
        "the question must be numbered after the comment it followed: {} then {}",
        earlier,
        only.id
    );
    // And the person's comment is still there: the merge added to the listing
    // rather than replacing it.
    assert_eq!(world.conversation().len(), 2, "{:?}", world.conversation());
}

/// The listing pages, and says so in the header a client follows.
///
/// Read out of the scripted `gh` itself rather than through a fiddle run, because
/// no capability this build can execute reads a conversation. That is a weaker
/// observation than a run following the header would be, and it is the strongest
/// one available: it proves the fixture offers `rel="next"`, not that fiddle
/// follows it. `fiddle-runtime`'s `human_comments` suite owns the following.
#[test]
fn the_conversation_pages_with_a_rel_next_link_header() {
    let world = World::new();
    let first = world.post_comment(AUTHORIZED, "one");
    let second = world.post_comment(AUTHORIZED, "two");
    world.paginate_conversation(1);

    let page_one = world.listing(1);
    assert!(
        page_one.contains(&format!("\"id\":{first}")),
        "page one must hold the first comment: {page_one}"
    );
    assert!(
        page_one.contains("rel=\"next\""),
        "a page with more after it must offer rel=\"next\": {page_one}"
    );

    let page_two = world.listing(2);
    assert!(
        page_two.contains(&format!("\"id\":{second}")),
        "page two must hold the second comment: {page_two}"
    );
    assert!(
        !page_two.contains("rel=\"next\""),
        "the last page must not offer rel=\"next\": {page_two}"
    );

    // And `conversation` really follows the header rather than stopping at the
    // first page. Written because an inversion that made it read page one only
    // broke nothing: every other test in the lane has a single-page conversation,
    // so the following was untested — and it is load-bearing for 565u, whose
    // question is merged by the stub onto the **last** page and would be invisible
    // to a reader that stopped early.
    assert_eq!(
        world
            .conversation()
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        [first, second],
        "the whole conversation is both pages, not the first of them"
    );
}

/// The `graphql` route answers in call order, and a 200 carrying `errors[]` is
/// one of the answers it can give.
///
/// The two facts are one test because they are one mechanism: the ending rides in
/// `graphql/<n>.json` precisely so that a refusal — which arrives as **200** with
/// an `errors[]` — can be scripted for call *two* while call one succeeds. A
/// route keyed on the request instead of on the call number could not express
/// that, and `fiddle-e902`'s property, that an uninterpretable 200 is unknown
/// rather than a success, would have no fixture to be asserted against.
#[test]
fn the_graphql_route_answers_in_call_order_including_a_200_carrying_errors() {
    let world = World::new();
    world.script_graphql(0, 200, serde_json::json!({"data": {"ok": true}}));
    world.script_graphql(
        1,
        200,
        serde_json::json!({"errors": [{"message": "refused"}]}),
    );

    assert_eq!(world.graphql_calls(), 0, "nothing has asked yet");

    let first = world.graphql("query { one }");
    assert!(first.contains("\"ok\":true"), "{first}");
    assert_eq!(world.graphql_calls(), 1);

    let second = world.graphql("query { two }");
    assert!(
        second.contains("HTTP/2.0 200"),
        "a refusal arrives with a 200 status line: {second}"
    );
    assert!(second.contains("refused"), "{second}");
    assert_eq!(world.graphql_calls(), 2);
}

/// Every path the forge was asked for is recorded, so a test can assert an
/// endpoint was **never** consulted.
///
/// The negative is the reason this exists, and a negative is worthless without
/// its denominator: this asserts the recorder saw the path that *was* asked for
/// in the same breath as the one that was not, so a recorder that recorded
/// nothing at all fails here rather than making every "never consulted"
/// assertion in this milestone pass for free.
#[test]
fn the_requested_paths_recorder_sees_what_was_asked_and_only_that() {
    let world = World::new();
    assert!(world.requested_paths().is_empty(), "nothing has asked yet");

    world.listing(1);

    let asked = world.requested_paths();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0].contains("/issues/"),
        "the recorded path must be the one that was asked for: {asked:?}"
    );
    assert!(
        !asked.iter().any(|path| path.contains("/pulls")),
        "no pull request endpoint was consulted, and the recorder must say so: {asked:?}"
    );
}

/// `the_only_request_comment` refuses a conversation that does not hold exactly
/// one of fiddle's questions, rather than answering about the first it finds.
///
/// The accessor exists for 565u, which reads the marker out of the question a
/// suspension published. Its cardinality *is* the claim — "the **only** request
/// comment" — because a run that asked twice is the defect a continuation exists
/// not to be, and a helper answering with the first of two would let that walk
/// pass.
///
/// # This test was written because an inversion found nothing
///
/// The first version of it asserted only over `request_comments`, and degrading
/// `the_only_request_comment` to `conversation()[0]` — no cardinality check, no
/// bot filter — broke **no test in the lane**. The accessor would have shipped to
/// 565u entirely unprotected. All three cases below are here as a result, and the
/// re-run of that same inversion is what says they close it.
///
/// The questions are seeded rather than published, because no capability this
/// build can execute publishes one; [`World::seed_question`] says so at length.
#[test]
fn the_only_request_comment_is_a_cardinality_assertion() {
    let world = World::new();
    assert!(
        world.request_comments().is_empty(),
        "an empty conversation holds no question"
    );
    assert!(
        panicked(|| {
            world.the_only_request_comment();
        }),
        "no question at all must be a refusal and not an answer"
    );

    // A person's comment is not fiddle's question, however much it looks like
    // one: the author is what distinguishes them, and `is_bot` is what
    // `validate::select_candidates` refuses on.
    world.post_comment(AUTHORIZED, "not a question fiddle asked");
    assert!(
        world.request_comments().is_empty(),
        "a person's comment must not count as fiddle's question: {:?}",
        world.request_comments()
    );
    assert!(
        panicked(|| {
            world.the_only_request_comment();
        }),
        "a conversation holding only a person's comment holds no question"
    );

    // One question, with a person's comment beside it. This is the case the
    // accessor is for, and the person's comment is what makes it about the
    // filter rather than about there being a single entry.
    let asked = world.seed_question("May fiddle mark it ready for review?");
    let only = world.the_only_request_comment();
    assert_eq!(only.id, asked);
    assert_eq!(only.body, "May fiddle mark it ready for review?");
    assert!(only.is_bot);

    // Two questions is the failure 565u would otherwise not see.
    world.seed_question("May fiddle ask that again?");
    assert!(
        panicked(|| {
            world.the_only_request_comment();
        }),
        "two questions must be a refusal: {:?}",
        world.request_comments()
    );
}

/// The two accessors the read-only scenario only ever asserts *empty* can see
/// something when there is something to see.
///
/// # Written because two inversions found nothing
///
/// `inspect_builds_nothing_for_propose_change` asserts `remote_branches` is empty
/// and `posted_comment_bodies` is empty, and those were the only assertions on either
/// accessor in the lane. So a `remote_branches` that answered `[]` unconditionally
/// and a `posted_comment_bodies` that answered `[]` unconditionally each broke **no
/// test** — both negatives were passing for free, and a future `inspect` that grew
/// a branch or a comment could have gone unnoticed.
///
/// The denominators are produced by fixture actions rather than by a run, because
/// no capability this build can execute pushes a branch or posts a comment for a
/// decision walk. That is weaker than a run would be in one specific way, said
/// here so nobody reads more into it: it proves each accessor reads the world it
/// claims to read — a real repository's refs, and the request log — not that
/// fiddle ever wrote there.
#[test]
fn the_accessors_the_read_only_scenario_asserts_empty_can_see_something() {
    let world = World::new();
    assert!(world.remote_branches().is_empty());
    assert!(world.posted_comment_bodies().is_empty());

    world.push_branch("fiddle/beans-m3-demo");
    assert_eq!(
        world.remote_branches(),
        ["fiddle/beans-m3-demo"],
        "the accessor must read the ref the remote really holds"
    );

    let answer = world.post_comment_through_the_forge("a body the recorder must see");
    assert!(
        answer.contains("HTTP/2.0 201"),
        "the write must have been accepted, or there is nothing to have recorded: \
         {answer}"
    );
    assert_eq!(
        world.posted_comment_bodies(),
        ["a body the recorder must see"],
        "the accessor must read the body that was really sent"
    );
}

/// A comment's `author` is the id that wrote it, and two different writers stay
/// two different ids.
///
/// # Written because a sixth inversion found nothing, and this was the worst one
///
/// `Comment::author` was asserted by **no test in the lane**: hardcoding the id
/// `post_comment` writes *and* the id `comment_from` reads, both at once, left all
/// nine tests passing. `grep -cE 'assert.*author'` over this file returned 0.
///
/// That is the field `[github.decision] authorized` matches on, so it is the
/// surface Task 14's whole authorization matrix reaches its verdict through. A
/// fixture that answered "the authorized user wrote it" whatever was written would
/// make every authorization test pass — including the ones whose entire point is a
/// stranger's reply being refused.
///
/// **Two authors and not one, because that is what defeats the inversion.** With a
/// single author, hardcoding either side of the round trip is invisible; with two
/// that must differ, hardcoding either side collapses them together and fails
/// here. `assert_ne!` on the constants is the denominator: it says the two ids were
/// distinguishable before the fixture was asked to distinguish them.
#[test]
fn the_author_of_a_comment_is_the_id_that_wrote_it() {
    assert_ne!(
        AUTHORIZED, STRANGER,
        "the two writers must be different people for anything below to be a test"
    );
    let world = World::new();
    world.post_comment(AUTHORIZED, "the nominated approver writes");
    world.post_comment(STRANGER, "somebody nobody nominated writes");
    world.seed_question("and fiddle asks");

    let conversation = world.conversation();
    assert_eq!(
        conversation
            .iter()
            .map(|comment| comment.author)
            .collect::<Vec<_>>(),
        [AUTHORIZED, STRANGER, FIDDLE_BOT],
        "each comment's author is the id that wrote it"
    );
    // And the bot flag is not a proxy for the author: the two people differ from
    // each other while sharing `is_bot == false`, which is what stops a fixture
    // deciding authorship from the flag.
    assert_eq!(
        conversation
            .iter()
            .map(|comment| comment.is_bot)
            .collect::<Vec<_>>(),
        [false, false, true]
    );

    // The tie to the document, which is what makes `author` load-bearing rather
    // than decorative: the id this world nominates is the id its own
    // `[github.decision]` table names.
    assert!(
        world
            .config_text()
            .contains(&format!("authorized = [{AUTHORIZED}]")),
        "the document must nominate the id the fixture writes: {}",
        world.config_text()
    );
}

/// A credential-free run really is credential-free: every variable this world's
/// own document names is removed from the child.
///
/// # Written because the guarantee was a claim about this machine
///
/// `CREDENTIAL_VARS` is four names and `FIDDLE_GITHUB_TOKEN` is not among them —
/// it is the variable this world's `[github]` table names, which is a property of
/// the fixture rather than a credential-shaped name in the wild. Nothing removed
/// it. So `fiddle_without_credentials` was passing because the *test runner*
/// happened not to export it, and `.env` in this worktree declares it. On a
/// machine where it is exported, `inspect_builds_nothing_for_propose_change` would
/// have been running a credentialled `inspect` while claiming the opposite.
///
/// **What this test can and cannot distinguish.** It asserts on the command this
/// world *builds*, not on a child's observed environment — no capability this
/// build can execute reaches the scripted `gh`, and the stub's environment
/// recorder is the only thing that can see a child's variables from outside. So it
/// proves the harness removes them, not that a running fiddle found none. That is
/// the right half to pin here: the defect was in the harness, and the removal is
/// what the whole read-only claim rests on.
///
/// The credentialled half is asserted from the **same list**, which is the point
/// of `WORLD_CREDENTIAL_VARS` existing: a variable added to the document must be
/// both set and removed, and one list cannot disagree with itself.
#[test]
fn a_credential_free_run_removes_every_variable_this_worlds_document_names() {
    // A token that is not the default, so this also shows `with_token_sentinel`
    // changes what is exported rather than being the identity.
    let token = "ghp_a_deliberately_different_value_e11a";
    let world = World::new().with_token_sentinel(token);

    let free = world.credential_environment(false);
    for name in support::WORLD_CREDENTIAL_VARS {
        assert_eq!(
            free.get(name),
            Some(&None),
            "a credential-free run must remove {name}, and it is the variable this \
             world's own document names: {free:?}"
        );
    }
    for name in support::CREDENTIAL_VARS {
        assert_eq!(
            free.get(name),
            Some(&None),
            "a credential-free run must remove {name}: {free:?}"
        );
    }

    let held = world.credential_environment(true);
    for name in support::WORLD_CREDENTIAL_VARS {
        assert_eq!(
            held.get(name),
            Some(&Some(token.to_string())),
            "a credentialled run must export {name}, from the same list that \
             removes it: {held:?}"
        );
    }
}

/// Whether `f` panicked, with its own message suppressed so a deliberate panic
/// does not litter the test output of a passing run.
fn panicked(f: impl FnOnce()) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    outcome.is_err()
}

// ---------------------------------------------------------------------------
// The property this lane cannot yet reach
// ---------------------------------------------------------------------------

/// **SPEC_DEFECT — the bean's suspended-path criterion has no surface in this
/// build, and the missing piece is another bean's.**
///
/// `fiddle-pwyi`'s criterion `m3-suspended-path-leaks-nothing` asks that the
/// credential sentinel appear in no stdout, no diagnostic, no published bundle
/// and — "the new surface in this milestone" — not in the comment a person reads.
/// Its Step 1 states it as a run of `--capability propose_change` expecting exit
/// 10.
///
/// That run does not exist. This test is the evidence, and it is an assertion
/// rather than a comment so that it **fails the day the wiring lands** and
/// somebody has to come back and write the real property:
///
/// - `run --capability propose_change` exits **2** with
///   `fiddle::capability::unbuildable`, not 10.
/// - So no question is published, so there is no comment a person reads, so the
///   one surface this criterion adds over M2's sentinel test cannot be observed.
/// - The three surfaces it shares with M2 — stdout, stderr, a published bundle —
///   are already proven adversarially by
///   `binary_repair::a_gateway_refusal_never_reaches_what_the_run_publishes`,
///   against a gateway that quotes the credential back in a response body.
///   Restating them here against a run that never holds a forge credential would
///   be a passing test of nothing, which is worse than the gap.
///
/// The wiring belongs to `fiddle-565u`: `main.rs:827`'s own comment says the
/// missing `EffectContext` and `DecisionTrace` belong to "the bean that gates a
/// suspension end to end", and 565u's Step 3 and Step 5 both claim it.
#[test]
fn the_suspended_path_is_not_yet_reachable_through_the_binary() {
    let world = World::new();
    let out = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        out.code,
        Some(2),
        "propose_change now runs — write the suspended-path sentinel assertion \
         this test is standing in for, and delete this test: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );
    assert!(
        out.stderr.contains("unbuildable"),
        "the refusal must be the construction one rather than a document \
         complaint: {}",
        out.stderr
    );
    // And the world is untouched, which is the half of the criterion that does
    // hold today: a refused construction publishes nothing anywhere.
    assert!(world.posted_comment_bodies().is_empty());
    assert_eq!(world.remote_branches().len(), 0);
    assert_eq!(world.all_published_bytes(), "");
}

/// A conversation entry is compared by what a reader can see, so a fixture that
/// grew a field cannot silently change what a test asserted.
#[allow(dead_code)]
fn describe(comment: &Comment) -> String {
    format!("{}: {:?}", comment.id, comment.body)
}
