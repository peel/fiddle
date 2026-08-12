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
//! # What this lane reaches, now that it reaches the whole walk
//!
//! `run --capability propose_change` executes. Until `fiddle-565u` it did not —
//! `build_capability`'s arm was `Err(Unbuildable { … })`, missing an
//! `EffectContext` whose worktree is the tree the attempt will *create* and a
//! `DecisionTrace` for the validation order to announce itself to — so this file
//! could prove the fixture and not the property. It now proves both:
//!
//! - the deletion helpers, each against its own denominator, which is what stops
//!   the walk below being vacuous;
//! - [`a_suspension_then_a_fresh_process_acts_only_on_what_the_conversation_says`],
//!   the test the milestone rests on, over three processes;
//! - and the four surfaces a credential must not reach, of which the fourth — the
//!   comment a person actually reads — could not be asserted until something
//!   published one.
//!
//! `the_suspended_path_is_not_yet_reachable_through_the_binary` was the tripwire
//! `fiddle-pwyi` left to fail on exactly that day. It has been replaced rather than
//! deleted, by
//! [`a_suspension_leaks_the_credential_on_no_surface_a_reader_reaches`].
//!
//! # One fixture step a scenario has to make, and why the stub cannot
//!
//! [`World::answer_pull_request_by_number`] supplies GitHub's by-number answer for
//! a pull request a run just created. The stub cannot derive it: a create body
//! carries a head *label*, a base and a title and **no revision**, so the one fact
//! `EnsurePullRequestReady` and the validation order both turn on is not something
//! the create could have told it. The value comes from the remote's own ref, read
//! with real git, and is asserted against the marker the run published — never from
//! anything fiddle printed.
//!
//! **There were two.** The second answered the by-id route the validation order's
//! step 5 reads through, and it is gone because that route now answers for itself,
//! from the two sources its own listing draws on. That the route could not was the
//! deepest finding on this milestone: because it panicked for any comment the
//! world's own `POST` created, **no continuation walk had ever been driven against a
//! posted question** — only against comments a test wrote into a by-id file on both
//! sides of a comparison. `gh_stub`'s `comment_by_id` carries the full reasoning and
//! why the scripted file still wins.
//!
//! The generalisation is worth more than either instance: **when a fixture has two
//! reads of one collection, closing one says nothing about the other, and the one
//! nobody has closed is the one the product depends on next.**

mod support;

use support::{
    a_real_repair, a_suspension_and_its_approval, interprets, parse_marker, Comment, World,
    AUTHORIZED, CONVERSATION_ISSUE, FIDDLE_BOT, INVOCATION_REF, SENTINEL, STRANGER,
};

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
// The walk: a suspension, a person, and a fresh process
// ---------------------------------------------------------------------------

/// The words the nominated approver writes. Named once because two things have to
/// agree about them: the comment on the conversation, and the span the model claims
/// to have copied out of it — which `interpret::decide` checks rather than trusts.
const APPROVAL: &str = "yes, go ahead";

/// What a suspended run leaves for the process that continues it.
///
/// Thin on purpose: a branch name and a binding are, with the `InvocationRef`, all a
/// second process is given. The `Run` is here only so a scenario can ask which
/// attempt this was — that answer lives in the bundle, and a scenario about attempt
/// ids needs the payload that names one.
struct Suspension {
    run: support::Run,
    branch: String,
    binding: support::Binding,
    /// The number the **world** gave the pull request A opened, read off the
    /// listing rather than assumed from a constant. Everything a continuation is
    /// asserted about is addressed by this.
    pull_request: u64,
}

/// Suspend a run and hand back what a fresh process would have to work from.
///
/// A helper rather than three copies, because every scenario below starts the same
/// way and the *starting* is not what any of them is about.
///
/// # It also does the two fixture steps the scripted `gh` cannot do for itself
///
/// Both are recorded at length on the helpers, and both are about the same gap: a
/// create tells the stub a head **label**, a base and a title, and no revision — so
/// GitHub's by-number answer for the pull request the run just opened has to be
/// supplied, from the revision the **remote really holds** rather than from anything
/// fiddle printed. And the by-id route the validation order's step 5 re-reads
/// through has no merge, deliberately, so the conversation has to be made
/// re-readable from what the listing really answered.
///
/// The mirror is **not** made here, and that is deliberate rather than an omission:
/// it has to happen after the reply a scenario seeds, or the reply has no entry and
/// the walk fails on the file it wanted. Getting that order wrong is how this helper
/// was first written, and the failure was a run that exited 11 with a `gh` that
/// could not answer — loud, but pointing at the wrong thing. The seed *is* made
/// here, because nothing a scenario does afterwards changes it.
///
/// Neither step invents a fact, and both are asserted rather than assumed: the
/// seeded revision is checked against the marker the run published *and* against the
/// remote's own ref.
fn suspend(world: &World) -> Suspension {
    let run = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        run.code,
        Some(10),
        "a run that asked a question waits: stdout={} stderr={}",
        run.stdout,
        run.stderr
    );

    let branches = world.remote_branches();
    assert_eq!(branches.len(), 1, "exactly one branch: {branches:?}");
    let branch = branches[0].clone();
    assert!(
        branch.starts_with("fiddle/"),
        "a branch fiddle published carries its namespace: {branch}"
    );

    // **The number the world gave A's pull request, read out of the world.** Not
    // taken from [`CONVERSATION_ISSUE`] and then compared against it — that is a
    // constant compared with itself — but read off the listing and *checked* against
    // it, which is a real property with a real way of failing.
    //
    // The two have to agree, and nothing else in this lane says so. The scripted
    // `gh` merges a run's posted comments onto the conversation listing **keyed on
    // the exact path**, so a question published against pull request 7 is only
    // visible to a read of `/issues/7/comments`. If the stub numbered pull requests
    // from anything other than 7, every read below would be looking at the wrong
    // conversation and would find no question at all — and the failure would look
    // like a run that never asked rather than like a fixture disagreeing with
    // itself. `CONVERSATION_ISSUE`'s own documentation states the coupling; this is
    // the assertion that holds it.
    let opened = world.open_pull_requests();
    assert_eq!(opened.len(), 1, "exactly one pull request: {opened:?}");
    let pull_request = opened[0]["number"]
        .as_u64()
        .unwrap_or_else(|| panic!("a listed pull request carries a number: {opened:?}"));
    assert_eq!(
        pull_request, CONVERSATION_ISSUE,
        "the conversation is read under {CONVERSATION_ISSUE} and the world numbered \
         this pull request {pull_request}; the stub merges a run's comments keyed on \
         the exact path, so these two disagreeing would hide the question rather \
         than report anything"
    );
    // **And it is a pull request for the branch A pushed, which is what says this
    // accessor reads the world at all.** An inversion is why this line exists: an
    // `open_pull_requests` that answered a fabricated `[{"number": 7, "state":
    // "open"}]` satisfied both the count and the number, so neither of those says
    // the listing was consulted — and the count is what the whole "no second pull
    // request" claim rests on. A fabrication that cannot also produce the branch A
    // derived fails here.
    //
    // It is a property worth having in its own right, not only a denominator: the
    // pull request a continuation finds has to be the one opened for *this run's*
    // branch, and the head is where the listing says so.
    assert_eq!(
        opened[0]["head"]["ref"].as_str(),
        Some(branch.as_str()),
        "the pull request must be the one opened for the branch this run published: \
         {opened:?}"
    );

    let binding = parse_marker(&world.the_only_request_comment().body)
        .expect("the question carries its marker");
    // The seed, and the assertion that ties it to the world rather than to this
    // file: the revision GitHub is told its pull request is at is the revision the
    // remote really holds, and it is the revision the run's own marker names.
    // Addressed by the number the *world* gave the pull request, so a fixture whose
    // numbering moved would seed the object the walk is about rather than a
    // neighbour of it.
    let seeded = world.answer_pull_request_by_number(pull_request, &branch);
    assert_eq!(
        seeded, binding.head_sha,
        "the head the forge answers with must be the head the question was asked \
         about, or a continuation is being asked about another change"
    );
    assert_eq!(
        seeded,
        world.remote_head(&branch),
        "and it is the commit the push really published"
    );

    Suspension {
        run,
        branch,
        binding,
        pull_request,
    }
}

/// **M3's central claim, and the reason every local record is deleted between the
/// two processes: a continuation that could read its own past would prove nothing
/// about a fresh one.**
///
/// Three processes, one work ref, and nothing between them but the conversation and
/// the forge.
///
/// # What each of the three is for
///
/// **A** produces a change, publishes it as a branch and a draft pull request, asks
/// the one question fiddle is not entitled to answer for itself, and stops on exit
/// 10. **B** is given the same `InvocationRef` and nothing else: it recomputes the
/// branch, finds its own pull request, finds its own question, validates the reply
/// against the binding it derives, marks the pull request ready, and records the
/// change set that says it accounted for the work. **C** finds there is nothing left
/// to do and does nothing — and *does nothing* is asserted as an empty execution
/// list rather than as an exit code, because that is the difference the change set
/// makes: C's pre-execution derivation reads B's marker, answers `complete`, and the
/// capability is never granted at all.
///
/// C used to walk the whole thing again every time — find the pull request, derive
/// the question, find the comment, and settle on a postcondition that already held —
/// because `propose_change` recorded no change set on any path. That is `fiddle-usp7`,
/// and while it stood C exited 11 having mutated nothing.
///
/// # Identity and not counts
///
/// Every object is asserted to be *the same object*, not one of a set of the right
/// size. A count alone is satisfied by close-and-reopen: a run that closed its own
/// pull request and opened a second would report one open pull request, one branch
/// and one question, having done the thing a continuation exists not to do. So the
/// branch is compared by name across all three processes, the pull request by number,
/// the question by the binding it carries, and the transition by the count of
/// GraphQL calls the world was asked to answer.
///
/// # The denominators, including the two that are honestly zero
///
/// The three deletions are all made, and what each of them had to delete is printed
/// rather than assumed. Two of them are **guards** on this path, and that is a fact
/// about a suspension rather than a weakness in the proof:
///
/// - a suspended run publishes a bundle, so `delete_report_bundles` deletes one;
/// - and having published one it **supersedes its own journal record** —
///   `journal.rs`'s `supersede` removes the file once the bundle lands — so there is
///   nothing for `delete_attempt_journal` to take;
/// - and `propose_change` drops its workspace explicitly after the push and the
///   question, so its worktree is already gone.
///
/// Each helper is proven against a non-zero denominator in
/// [`deleting_the_local_past_really_deletes_it`], which is where two runs are
/// arranged precisely so that all three have something to delete. Here the point is
/// that all three are *called* and that nothing local survives them — and the
/// counts below are what stop "found nothing" and "looked at nothing" being the same
/// observation.
#[test]
fn a_suspension_then_a_fresh_process_acts_only_on_what_the_conversation_says() {
    let world = World::with_model_script(a_suspension_and_its_approval(APPROVAL));

    // --- process A: propose, ask, and stop ---
    let Suspension {
        branch,
        binding,
        pull_request,
        ..
    } = suspend(&world);
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(true),
        "it was opened as a draft, because the transition out of one is the gated act"
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "a run that asked a question has spent no approval"
    );

    // --- the past is deleted, all of it, and each helper's denominator is said ---
    let bundles = world.report_bundles().len();
    let records = world.journal_records().len();
    let worktrees = world.worktrees().len();
    assert!(
        bundles > 0,
        "a suspended run publishes a bundle like any other, and {} holds none",
        world.report_dir().display()
    );
    // Stated as the numbers they are rather than asserted non-zero, because on this
    // path two of them are zero *for a reason*. See this test's own documentation;
    // an assertion here would be an assertion that a suspension leaves litter it is
    // designed not to leave.
    assert!(
        !world.local_state_is_empty(),
        "there must be something to delete: {bundles} bundles, {records} journal \
         records, {worktrees} workspace entries"
    );

    world.delete_report_bundles();
    world.delete_attempt_journal();
    world.delete_workspaces();
    assert!(
        world.local_state_is_empty(),
        "the second process must have nothing to read, and it can still see {:?}, \
         {:?}, {:?}",
        world.report_bundles(),
        world.journal_records(),
        world.worktrees()
    );

    // --- a person answers ---
    world.post_comment(AUTHORIZED, APPROVAL);
    world.accept_the_ready_mutation();
    // **No fixture step between the reply and the continuation, and that absence is
    // the point.** The validation order's step 5 re-reads the request comment and
    // every candidate by id, and the scripted `gh` now answers that route from the
    // two sources its own listing draws on — the page a person's reply was written
    // to, and the `POST` fiddle's question really made. This scenario used to have
    // to mirror the conversation into by-id files first, and a step a test has to
    // remember is a step a test can forget.

    // --- process B: a fresh process, given only the same InvocationRef ---
    let b = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    // Zero, and it used to be 11. `fiddle-usp7` is why: the capability completed and
    // recorded no change set, so the post-execution re-derivation found the work
    // `not_started` and concluded *try again* over a transition that had landed. The
    // exit code is asserted here alongside the effect, never instead of it — see the
    // note on `graphql_calls` below for what an exit code on this path could not
    // distinguish while that defect stood.
    assert_eq!(
        b.code,
        Some(0),
        "a fresh process must continue and conclude: stdout={} stderr={}",
        b.stdout,
        b.stderr
    );
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(false),
        "it was marked ready, and the forge is what says so"
    );
    // **The denominator for C's empty execution list**, and the reason it is read
    // here rather than asserted there alone: `capability_executions == []` is only a
    // claim about C if the same field is *non*-empty for a process that did execute.
    // B is that process, and this is the one place in the walk where both payloads
    // are in hand.
    let executed: serde_json::Value = serde_json::from_str(&b.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", b.stdout));
    assert_eq!(
        executed["capability_executions"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        1,
        "B executed the capability once, so an empty list below is a difference \
         rather than the shape this field always has: {executed}"
    );

    // Exactly one of each object, and the same ones: identity, not counts.
    assert_eq!(
        world.remote_branches(),
        [branch.as_str()],
        "the same branch"
    );
    // **This line used to be `pull_request(CONVERSATION_ISSUE)["number"] ==
    // CONVERSATION_ISSUE`, which cannot fail** — both sides trace to one constant,
    // because `answer_pull_request_by_number` writes the number it is given and the
    // stub's landed-transition rewrite touches `draft` and nothing else. An
    // evaluator measured it rather than arguing it: the assertion passed when made
    // before B existed at all. It is the same shape as the title-clock comment
    // caught earlier in this bean — a comment claiming more than its line does.
    //
    // It is **removed rather than replaced by a fifth check**, because the pull
    // request's identity is already carried, and a made-up fifth would be the same
    // decoration in a new spelling. Where it actually lives:
    //
    // - **the cardinality** below — a run that closed its own pull request and
    //   opened a second is what a bare count would miss, and this is that count;
    // - **`draft` flipping to `false`** above, read from a stub that takes a pull
    //   request out of draft only when a landed mutation names *its own node id*, so
    //   the object B acted on is the object A opened;
    // - **the number itself**, checked against the conversation constant in
    //   `suspend` where the coupling matters, off the world's own listing.
    assert_eq!(
        world.open_pull_requests().len(),
        1,
        "one pull request, not a second alongside it: {:?}",
        world.open_pull_requests()
    );
    assert_eq!(
        world.comments_naming(&binding.request).len(),
        1,
        "no second question: {:?}",
        world.comments_naming(&binding.request)
    );
    // **The assertion that survives a change of exit code, and is kept for that
    // reason.** `fiddle-565u` added it while `usp7` stood, because 11 then meant both
    // *B did the transition* and *B could not read the conversation* — three of its
    // inversions over the by-id read came back green against a test that asserted
    // only the number. Zero is unambiguous where 11 was not, and this line stays
    // anyway: a test that reads the effect out of the world does not have to be
    // revisited the next time an exit code changes meaning, and the last one did.
    assert_eq!(
        world.graphql_calls(),
        1,
        "one ready transition was dispatched, and only one"
    );

    // And B derived the identity rather than remembering it: the binding it
    // validated against is the one A published, all four fields of it. B had no
    // bundle, no journal and no workspace to have remembered it from.
    assert_eq!(
        parse_marker(&world.the_only_request_comment().body).unwrap(),
        binding,
        "the binding B validated against is the one A published",
    );

    // --- process C: nothing left to do, and nothing done ---
    //
    // **Asked for `--json`, which it was not before, and that is the substance of
    // this change rather than the exit code beside it.** While `fiddle-usp7` stood C
    // exited 11 *having executed the capability* — it walked the forge, found the
    // question, and settled on a postcondition that already held — and 11 was
    // asserted with nothing else, so "found nothing to do" was a sentence in the
    // message and not a claim the test made. Flipping 11 to 0 alone would have
    // reproduced that: an exit code cannot tell *C was never granted* from *C ran
    // and concluded*.
    //
    // What the marker changes is exactly that. C's **pre-execution** derivation
    // reads the change set B wrote, answers `complete`, and no grant is issued — so
    // the observation that says so is an empty execution list, and it is read off
    // the payload rather than inferred.
    let c = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        c.code,
        Some(0),
        "a third process must find nothing to do: stdout={} stderr={}",
        c.stdout,
        c.stderr
    );
    let payload: serde_json::Value = serde_json::from_str(&c.stdout)
        .unwrap_or_else(|error| panic!("stdout is not JSON ({error}): {}", c.stdout));
    assert_eq!(
        payload["capability_executions"],
        serde_json::json!([]),
        "C executed nothing: the derivation it made before executing was already \
         `complete`, so no grant was issued: {payload}"
    );
    assert_eq!(
        payload["next_action"],
        serde_json::json!("complete"),
        "and that is what it derived, off the change set B recorded: {payload}"
    );
    assert_eq!(
        payload["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(world.expected_marker(INVOCATION_REF)),
        "the marker it completed on is this run's own, recomputed here from the two \
         inputs it is derived from rather than read back out of the same payload: \
         {payload}"
    );
    assert_eq!(
        world.graphql_calls(),
        1,
        "the mutation is not repeated: the postcondition already holds"
    );
    assert_eq!(world.open_pull_requests().len(), 1);
    assert_eq!(
        world.remote_branches(),
        [branch.as_str()],
        "still the same branch"
    );
    assert_eq!(
        world.comments_naming(&binding.request).len(),
        1,
        "and still one question"
    );
}

/// A different attempt id each time, the same work ref throughout — M2's
/// neighbouring property, restated for a walk that spans three processes.
///
/// The two halves are one test because they are one claim: *these are two attempts
/// at one piece of work*. An assertion that the attempt ids differ, on its own, is
/// satisfied by two runs about entirely different things; an assertion that the work
/// refs agree, on its own, is satisfied by one run asserted against itself.
#[test]
fn each_process_is_its_own_attempt_against_one_work_ref() {
    let world = World::with_model_script(a_suspension_and_its_approval(APPROVAL));
    let suspended = suspend(&world);
    let (a, pull_request) = (suspended.run, suspended.pull_request);

    world.post_comment(AUTHORIZED, APPROVAL);
    world.accept_the_ready_mutation();
    let b = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    // Zero, and it was 11 until `fiddle-usp7` landed. Neither number is what this
    // test is about — a run exits, mints its own attempt id and publishes a bundle
    // naming the work on either code — but a continuation that did not conclude is
    // not the run whose attempt id this test then compares, so the code is asserted
    // as the precondition it is.
    assert_eq!(b.code, Some(0), "stdout={} stderr={}", b.stdout, b.stderr);
    // **And the exit code alone was not enough to say B continued, which an
    // inversion is what taught.** While `usp7` stood, row 11 was what a *successful*
    // continuation earned — and also what a continuation that refused at step 5
    // earned, because an unreadable comment is an adapter failure and adapter
    // failures are retryable too. One number, two outcomes. Three inversions over
    // the by-id route came back null against this test for exactly that reason,
    // while the three-process walk caught all three.
    //
    // Row 0 does not carry that ambiguity: a refusal at step 5 is retryable and
    // cannot reach it. **These two lines stay regardless**, and the reason is the
    // finding rather than caution — the ambiguity was invisible from inside the test
    // that had it, so the defence is to assert the effect and not only the code,
    // whatever the code currently means. Both are read out of the world rather than
    // out of B's own payload.
    assert_eq!(
        world.graphql_calls(),
        1,
        "B must have continued rather than merely failed retryably"
    );
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(false),
        "and the forge is what says the transition happened"
    );

    assert_ne!(
        world.attempt_id(&a),
        world.attempt_id(&b),
        "each process is its own attempt"
    );
    assert_eq!(
        world.work_ref(&a),
        world.work_ref(&b),
        "and both are about the same work"
    );
    // **The line above needs this one, and an inversion is what said so.** A
    // `work_ref` accessor that answered the same thing whatever bundle it was
    // handed passes an equality between two of its own calls — the sharpened rule
    // exactly, a value appearing only where its value cannot matter. So it is also
    // checked against something outside itself: `work_ref` is the invocation
    // reference, which `orchestration.rs` says outright and says why — *"the
    // stability proof compares `work_ref` across two attempts, which would prove
    // nothing if it were derived from the attempt"*.
    assert_eq!(
        world.work_ref(&a),
        INVOCATION_REF,
        "and the work they are both about is the one the caller named"
    );
}

// ---------------------------------------------------------------------------
// The credential, on the four surfaces a reader reaches
// ---------------------------------------------------------------------------

/// **The credential reaches no surface a reader can reach — including the one this
/// milestone adds, the comment a person actually reads.**
///
/// This replaces `the_suspended_path_is_not_yet_reachable_through_the_binary`, the
/// tripwire `fiddle-pwyi` left in its place. That test pinned exit **2** and
/// `stderr.contains("unbuildable")` and existed to fail the day the wiring landed,
/// which it now has: `run --capability propose_change` builds, suspends, and exits
/// 10. The property it was standing in for is the one below, and `fiddle-pwyi`'s
/// confirming pass specified the four surfaces by name.
///
/// # The denominator, which is the whole reason this is not four vacuous searches
///
/// A search for a string that was never anywhere near this process passes for free.
/// So the first assertion is that the sentinel really was handed to the children:
/// the scripted `gh` and the recording `git` each write down the entire environment
/// they were given, **by design** — that recording is how the credential-free and
/// credentialled assertions elsewhere are made at all — so the sentinel is provably
/// in this world, in a place a reader does not reach, before anything is said about
/// the places a reader does.
///
/// # And one surface where the check is honestly weak, said here rather than implied
///
/// A suspended run writes nothing to stderr — there is no diagnostic on this path,
/// only an outcome — so the search over it is a search over an empty string. Its
/// length is asserted rather than its content, so a reader is not left thinking the
/// stream was examined and found clean. The adversarial version of that surface is
/// `binary_repair::a_gateway_refusal_never_reaches_what_the_run_publishes`, against
/// a gateway that quotes the credential back in a response body; what is new here is
/// the fourth surface, which nothing could assert until a run published a question.
#[test]
fn a_suspension_leaks_the_credential_on_no_surface_a_reader_reaches() {
    let world = World::with_model_script(a_suspension_and_its_approval(APPROVAL));
    let out = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        out.code,
        Some(10),
        "the run must have reached the forge and asked: stdout={} stderr={}",
        out.stdout,
        out.stderr
    );

    // The denominator: the sentinel is in this world, where it belongs.
    let recorded = world.requests();
    assert!(
        !recorded.is_empty(),
        "the forge must have been reached, or there is nothing to search"
    );
    assert!(
        recorded
            .iter()
            .any(|request| request.to_string().contains(SENTINEL)),
        "the fixture records every child's environment on purpose, so the \
         credential must be findable there — otherwise every search below passes \
         for free"
    );

    // 1. stdout: the payload a caller reads.
    assert!(
        !out.stdout.is_empty(),
        "a run prints its payload on every path, including this one"
    );
    assert!(
        !out.stdout.contains(SENTINEL),
        "the credential reached stdout: {}",
        out.stdout
    );

    // 2. stderr. Empty on this path, and the assertion says so rather than
    //    pretending to have examined something.
    assert_eq!(
        out.stderr.len(),
        0,
        "a suspended run writes no diagnostic, so the search below is over nothing: \
         {}",
        out.stderr
    );
    assert!(!out.stderr.contains(SENTINEL));

    // 3. Every byte of every document the run published.
    let published = world.all_published_bytes();
    assert!(
        published.contains(INVOCATION_REF),
        "the published bytes must be this run's, and they are {} long",
        published.len()
    );
    assert!(
        !published.contains(SENTINEL),
        "the credential reached a published bundle"
    );

    // 4. **The comment a person actually reads** — the surface this milestone adds,
    //    and the one nothing could assert until a run published a question.
    let question = world.the_only_request_comment();
    assert!(
        question.body.contains("ready for review"),
        "the question must be the one a person is meant to answer: {}",
        question.body
    );
    assert!(
        !question.body.contains(SENTINEL),
        "the credential reached the comment a person reads: {}",
        question.body
    );
}

// ---------------------------------------------------------------------------
// The marker, re-derived rather than borrowed
// ---------------------------------------------------------------------------

/// `parse_marker` is as strict as the design says, so a body that merely resembles a
/// request comment is not read as one.
///
/// # Why this test has to exist
///
/// The walk above compares two bindings and asserts they are equal. **A
/// `parse_marker` that returned the same thing whatever it was handed would pass
/// that assertion**, which is the sharpened rule exactly: a value that only appears
/// where its value cannot matter is not tested. So the parser needs a case where its
/// answer is checked against something else, and cases where it must refuse.
///
/// The positive half is checked in the walk itself and by the first case here: the
/// `head` a marker names is compared against the revision the remote really holds,
/// which is a fact from outside this file.
///
/// The negatives are each a way a body can look like a marker without being one, and
/// the design names the set: *"the exact key order, the exact lengths, lowercase
/// hex, and no extra keys"*. A lenient parser here would let a scenario assert that
/// a question was published against something that is not a question.
#[test]
fn the_marker_grammar_is_read_exactly_as_the_design_states_it() {
    let request = "a".repeat(16);
    let effect = "b".repeat(16);
    let payload = "c".repeat(16);
    let head = "d".repeat(40);
    let good = format!(
        "May fiddle mark it ready?\n\n<!-- fiddle:decision v1 request={request} \
         effect={effect} payload={payload} head={head} -->"
    );

    let binding = parse_marker(&good).expect("the canonical form parses");
    // Each field read into the field it names, and not a permutation of them: four
    // values of three distinct widths, so a parser that swapped two of the 16-wide
    // ones is caught by their contents rather than by their lengths.
    assert_eq!(binding.request, request);
    assert_eq!(binding.effect, effect);
    assert_eq!(binding.payload, payload);
    assert_eq!(binding.head_sha, head);

    // A body with no marker at all is the ordinary case — a person's reply carries
    // none — and it is a refusal rather than an empty binding.
    assert!(parse_marker("yes, go ahead").is_err());

    for (why, body) in [
        (
            "two markers is not a body to choose between",
            format!("{good}\n{good}"),
        ),
        (
            "a marker that never closes",
            format!("<!-- fiddle:decision v1 request={request} effect={effect} payload={payload} head={head}"),
        ),
        (
            "a version from another build",
            good.replace("v1", "v2"),
        ),
        (
            "the keys out of the one order they may be spelled in",
            format!(
                "<!-- fiddle:decision v1 effect={effect} request={request} \
                 payload={payload} head={head} -->"
            ),
        ),
        (
            "a fifth key",
            good.replace(" -->", &format!(" actor={request} -->")),
        ),
        (
            "a field one character short",
            good.replace(&head, &"d".repeat(39)),
        ),
        (
            "uppercase hex, which is not the rendering",
            good.replace(&request, &"A".repeat(16)),
        ),
        (
            "a value that is not hex at all",
            good.replace(&request, &"z".repeat(16)),
        ),
        (
            "a doubled space, which is what a reflowed body leaves behind",
            good.replace("v1 request", "v1  request"),
        ),
    ] {
        assert!(
            parse_marker(&body).is_err(),
            "{why} must be refused, and was read as {:?}",
            parse_marker(&body)
        );
    }
}

/// A conversation entry is compared by what a reader can see, so a fixture that
/// grew a field cannot silently change what a test asserted.
#[allow(dead_code)]
fn describe(comment: &Comment) -> String {
    format!("{}: {:?}", comment.id, comment.body)
}

// ---------------------------------------------------------------------------
// The matrix: every way the answer is not a plain approval
// ---------------------------------------------------------------------------

/// One row of the matrix.
///
/// The reply is a function of the world rather than a value, because the eight rows
/// differ in *which collection and which authorship* a reply arrives under — a
/// person's comment, a bot's, an app's, an inline review comment, none — and those
/// are different acts against the fixture rather than different strings.
struct Row {
    name: &'static str,
    /// The verdict scripted for step 7, and the span the model claims to have copied
    /// out of the reply.
    ///
    /// **Every row scripts one, including the five whose walk must never reach step
    /// 7.** That is the same principle as arming the GraphQL mutation: the world is
    /// made *willing*, so a refusal is fiddle's rather than the fixture's. The five
    /// arm it to **approve** — the most dangerous answer available — so a walk that
    /// wrongly took a bot's or a stranger's comment as a candidate is answered
    /// "approve" and mutates, and the row fails on the forge.
    ///
    /// This replaced an `Option` that was `None` on those five rows, which was
    /// **documented as an assertion and was not one**: the gateway drops its listener
    /// when the script runs out, but `interpret` collapses every transport failure to
    /// `Unclear` (`human/interpret.rs:266-271`), and `Unclear` is exit 10 with nothing
    /// mutated — identical to the reply having been refused. Two inversions came back
    /// null against it. See [`World::model_calls`].
    interpretation: (&'static str, &'static str),
    /// What the conversation says when the continuation wakes.
    reply: fn(&World),
    exit: i32,
    /// Whether the pull request is out of draft afterwards — read from the forge.
    ready: bool,
    /// How many completions the whole row spends: **two** for the suspension's
    /// bounded attempt, plus one if and only if the walk reached step 7.
    ///
    /// The field that makes "no model was reached" a claim this table makes rather
    /// than one its documentation asserts. Three says a reply was interpreted; two says
    /// the six deterministic steps disposed of the conversation without a model
    /// existing, which for an authorization matrix is the point.
    model_calls: usize,
}

/// Every row asserts against the **remote**, and that is the whole design of this
/// test.
///
/// A report claiming no mutation beside a pull request that is ready is exactly the
/// failure this milestone exists to prevent, so fiddle's own account of what it did
/// cannot be the evidence for what it did. Each row therefore reads `draft` back out
/// of the forge's own by-number answer and counts the GraphQL calls the world was
/// asked to answer. The exit code is asserted beside those and never instead of them
/// — `fiddle-usp7` is the reason that distinction is written down rather than
/// assumed, because while it stood exit 11 meant both *the transition happened* and
/// *the conversation could not be read*.
///
/// # The mutation is armed on every row, including the seven that must not mutate
///
/// [`World::accept_the_ready_mutation`] is called unconditionally, and that is the
/// load-bearing choice in this test. The GraphQL route has no unscripted default: a
/// run that wrongly dispatched the transition against an unarmed world would fail
/// inside the stub, leave `draft` true, and **pass** every assertion below while
/// having attempted precisely the act the row forbids.
///
/// Armed, the world is *willing*. A wrongly dispatched mutation is answered, the stub
/// rewrites the pull request out of draft, and the row fails on `draft` — which is
/// the failure the milestone is about, reported as itself. "Still a draft" now means
/// fiddle refused, rather than meaning the fixture did.
const MATRIX: &[Row] = &[
    Row {
        name: "plain approval",
        interpretation: ("approve", APPROVAL),
        reply: |world| {
            world.post_comment(AUTHORIZED, APPROVAL);
        },
        exit: 0,
        ready: true,
        model_calls: 3,
    },
    Row {
        name: "rejection",
        interpretation: ("reject", "no, drop this"),
        reply: |world| {
            world.post_comment(AUTHORIZED, "no, drop this");
        },
        // 20 and not 10: a person said no, and `Recurrence::Permanent` is the row
        // that tells a caller there is nothing here for a repeat to get past. A
        // rejection is the one non-approval that *concludes* the run.
        exit: 20,
        ready: false,
        model_calls: 3,
    },
    Row {
        name: "unclear",
        interpretation: ("unclear", "what does this change?"),
        reply: |world| {
            world.post_comment(AUTHORIZED, "what does this change?");
        },
        // 10, because the question still stands. This is the row that says a
        // non-approval is not automatically a failure.
        exit: 10,
        ready: false,
        model_calls: 3,
    },
    Row {
        name: "unauthorized actor",
        interpretation: ("approve", "approve"),
        reply: |world| {
            world.post_comment(STRANGER, "approve");
        },
        exit: 10,
        ready: false,
        model_calls: 2,
    },
    // The two spellings of not being a person, and **both written by `AUTHORIZED`**.
    // That is deliberate and it is what makes the two rows mean anything: a bot reply
    // from `STRANGER` would be declined by the allowlist, so the row would keep
    // passing with the personhood check deleted from the product outright. Written by
    // the nominated approver, the only thing between each of these and a mutation is
    // its authorship.
    Row {
        name: "bot author",
        interpretation: ("approve", "approve"),
        reply: |world| {
            world.post_bot_comment(AUTHORIZED, "approve");
        },
        exit: 10,
        ready: false,
        model_calls: 2,
    },
    Row {
        name: "app author",
        interpretation: ("approve", "approve"),
        reply: |world| {
            world.post_app_comment(AUTHORIZED, "approve");
        },
        exit: 10,
        ready: false,
        model_calls: 2,
    },
    Row {
        name: "review comment only",
        interpretation: ("approve", "approve"),
        reply: |world| {
            world.post_review_comment(AUTHORIZED, "approve");
        },
        exit: 10,
        ready: false,
        model_calls: 2,
    },
    Row {
        name: "no reply at all",
        interpretation: ("approve", "approve"),
        reply: |_| {},
        exit: 10,
        ready: false,
        model_calls: 2,
    },
];

#[test]
fn the_decision_matrix_mutates_only_where_it_should() {
    // The denominator for the whole table: one row mutates and seven do not, so a
    // product that refused everything — or accepted everything — fails here rather
    // than satisfying a table that only ever asserted refusals.
    assert_eq!(
        MATRIX.iter().filter(|row| row.ready).count(),
        1,
        "exactly one row may mutate, or this table is a bias rather than a rule"
    );
    assert_eq!(MATRIX.len(), 8);

    for row in MATRIX {
        let name = row.name;
        let (verdict, evidence) = row.interpretation;
        let mut script = a_real_repair();
        script.push(interprets(verdict, evidence));
        let world = World::with_model_script(script);

        let suspension = suspend(&world);
        let pr = suspension.pull_request;
        (row.reply)(&world);
        world.accept_the_ready_mutation();

        let run = world.fiddle([
            "run",
            "--capability",
            "propose_change",
            INVOCATION_REF,
            "--json",
        ]);

        // The world first, and the exit code second, because the world is the claim.
        assert_eq!(
            world.pull_request(pr)["draft"],
            serde_json::json!(!row.ready),
            "{name}: the forge's own answer for pull request {pr} disagrees with this \
             row; stdout={} stderr={}",
            run.stdout,
            run.stderr
        );
        assert_eq!(
            world.graphql_calls(),
            usize::from(row.ready),
            "{name}: the number of ready transitions dispatched, counted by the world \
             that would have answered them"
        );
        // **Was a model reached at all.** Two means the six deterministic steps
        // disposed of this conversation and step 7 never ran; three means a reply was
        // interpreted. Without this the five refusing rows could not tell a reply that
        // was declined from one that was accepted and then failed to interpret, and an
        // inversion over each of `post_bot_comment` and `post_app_comment` came back
        // null for exactly that reason.
        assert_eq!(
            world.model_calls(),
            row.model_calls,
            "{name}: completions served by the endpoint; the suspension spends two and \
             step 7 spends the third"
        );
        assert_eq!(
            run.code,
            Some(row.exit),
            "{name}: stdout={} stderr={}",
            run.stdout,
            run.stderr
        );

        if name == "review comment only" {
            // **The reply is where a reply would count, and it still did not.** Without
            // this, [`FIRST_REVIEW_COMMENT`]'s value appears only in a position where
            // any value would do — the sharpened rule exactly — because "the endpoint
            // was not consulted" is true of a comment numbered anywhere.
            //
            // `select_candidates` silently skips any comment whose id is *below* the
            // request comment's, so a review comment numbered under the question could
            // be merged onto the conversation by a future defect and still change
            // nothing. These two assertions say it is numbered above the question and
            // absent from the conversation: a comment that *would* be a candidate, and
            // is not one, which is what makes the negative above a property rather
            // than an accident of numbering.
            let question = world.the_only_request_comment();
            assert!(
                support::FIRST_REVIEW_COMMENT > question.id,
                "the review comment must be numbered above the question ({} vs {}), or \
                 a defect merging the collection would change nothing and this row \
                 could not fail",
                support::FIRST_REVIEW_COMMENT,
                question.id
            );
            assert!(
                !world
                    .conversation()
                    .iter()
                    .any(|comment| comment.body == "approve"),
                "{name}: the review comment must not be on the conversation: {:?}",
                world.conversation()
            );

            // **Not merely ignored — never consulted.** An approval sitting in the
            // review-comment collection must be unreachable rather than filtered, so
            // the assertion is about the request that was not made.
            //
            // With its denominator, because "found nothing" and "examined nothing"
            // must not look alike: the conversation's own endpoint is asserted to be
            // among the paths in the same breath, so a recorder that recorded nothing
            // fails here instead of making this negative pass for free.
            let paths = world.requested_paths();
            assert!(
                paths
                    .iter()
                    .any(|path| path.contains(&format!("/issues/{pr}/comments"))),
                "{name}: the conversation must have been read, or this negative \
                 examined nothing; {} paths recorded: {paths:?}",
                paths.len()
            );
            assert!(
                !paths
                    .iter()
                    .any(|path| path.contains(&format!("/pulls/{pr}/comments"))),
                "{name}: the review-comment endpoint was consulted; {} paths \
                 recorded: {paths:?}",
                paths.len()
            );
        }
    }
}

/// And the converse of the last-reply rule, so it is demonstrated as a rule rather
/// than as a bias toward refusing.
///
/// The two halves are one test because neither is worth much alone. *Approve then
/// refuse does not mutate* is satisfied by a product that never mutates; *refuse then
/// approve does mutate* is satisfied by one that always does. Only the pair says the
/// **last** authorized reply decides, and the two runs are identical but for the order
/// of two comments.
///
/// Two worlds and not one, because a walk is spent when it concludes: the approving
/// half marks the pull request ready, and a second walk against the same world would
/// be a different scenario — the one `already_ready` answers.
///
/// # The evidence span is what pins *which* reply was read, and it was worth a defect
///
/// `interpret::decide` refuses a model that quoted a span the reply it was handed does
/// not contain. So scripting the verdict against the **retraction's** words is not
/// decoration: a walk that had chosen the earlier approval instead would be handed
/// `"yes, go ahead"`, would not find `"wait, no — hold off"` inside it, and would come
/// back `Unclear` — a different exit and no mutation for an entirely different reason.
/// The two comments therefore cannot be silently swapped underneath this test.
///
/// This was written the wrong way first, and the wrong way passed nothing:
/// [`a_suspension_and_its_approval`] always scripts **approve** and takes only the
/// span, so handing it the retraction's words scripted *"approve, quoting the
/// retraction"* — and the run duly marked the pull request ready. The failure was the
/// correct one. It is recorded here because the helper's name reads like it takes a
/// verdict and does not.
#[test]
fn the_last_authorized_reply_decides_in_both_directions() {
    // Approve, then change your mind. The verdict is scripted explicitly rather than
    // through `a_suspension_and_its_approval`, which only ever approves.
    let mut retracting = a_real_repair();
    retracting.push(interprets("reject", "wait, no — hold off"));
    let held_off = World::with_model_script(retracting);
    let first = suspend(&held_off);
    held_off.post_comment(AUTHORIZED, APPROVAL);
    held_off.post_comment(AUTHORIZED, "wait, no — hold off");
    held_off.accept_the_ready_mutation();
    let stopped = held_off.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        held_off.pull_request(first.pull_request)["draft"],
        serde_json::json!(true),
        "the last authorized reply decides, and it said no: stdout={} stderr={}",
        stopped.stdout,
        stopped.stderr
    );
    assert_eq!(
        held_off.graphql_calls(),
        0,
        "an approval a person withdrew before the run woke is not spent"
    );
    // 20, the rejection row: `Recurrence::Permanent`, because a person said no and
    // there is nothing here for a repeat to get past.
    assert_eq!(
        stopped.code,
        Some(20),
        "stdout={} stderr={}",
        stopped.stdout,
        stopped.stderr
    );

    // Refuse, then think better of it.
    let went_ahead =
        World::with_model_script(a_suspension_and_its_approval("actually yes, go ahead"));
    let second = suspend(&went_ahead);
    went_ahead.post_comment(AUTHORIZED, "not like that");
    went_ahead.post_comment(AUTHORIZED, "actually yes, go ahead");
    went_ahead.accept_the_ready_mutation();
    let proceeded = went_ahead.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        proceeded.code,
        Some(0),
        "stdout={} stderr={}",
        proceeded.stdout,
        proceeded.stderr
    );
    assert_eq!(
        went_ahead.pull_request(second.pull_request)["draft"],
        serde_json::json!(false),
        "a refusal a person reversed does not stand in the way"
    );
    assert_eq!(went_ahead.graphql_calls(), 1);
}

/// The head moved, so the effect the approval named no longer exists — and the
/// approval is not rejected, it is **never seen**.
///
/// # What the run really does, measured rather than assumed
///
/// The bean's note says "refused at step 3 on identity", and the walk is both more
/// interesting than that and not a refusal at all. Measured: the run exits **10**
/// having published a *second question*, about the head that now exists.
///
/// The mechanism is the identity derivation. The gated effect's target is
/// `{repo}#{pr}@{head}` and the request id is derived over it, so a run reading a
/// different head derives a **different request id**. `PublishDecisionRequest`'s own
/// `inspect` then finds no comment on the conversation carrying *that* marker,
/// answers `None`, and the capability takes the first walk rather than the
/// continuation: it asks. No step of the validation order runs at all, so neither
/// `RequestAbsent` (step 2) nor `HeadMoved` (step 6) is reached.
///
/// **This is what the test's name claims and a stronger version of it.** An approval
/// for a superseded head is not weighed and declined; it is unrecognisable as an
/// answer to any question this run knows how to ask, and the run's response is to ask
/// a fresh one. The two questions sit on the conversation naming two request ids and
/// two heads, and nothing was spent on either.
///
/// The old attempt is not re-run — the branch and the pull request already exist, so
/// `tools:0` — which is why the model script needs no second repair.
#[test]
fn an_approval_for_a_head_that_has_moved_is_unrecognisable_not_merely_rejected() {
    let world = World::with_model_script(a_suspension_and_its_approval(APPROVAL));
    let suspension = suspend(&world);
    world.post_comment(AUTHORIZED, APPROVAL);
    world.accept_the_ready_mutation();

    let moved_to = "deadbeef".repeat(5);
    let was = world.move_pull_request_head(suspension.pull_request, &moved_to);
    assert_eq!(
        was, suspension.binding.head_sha,
        "the revision that moved must be the one the question was asked about, or \
         this scenario moved something else"
    );

    let run = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);

    // The world, first and always: the transition did not happen and nothing was
    // spent trying, against a world that was armed to accept it.
    assert_eq!(
        world.pull_request(suspension.pull_request)["draft"],
        serde_json::json!(true),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "an approval naming a head that has gone is not spent on the head that \
         replaced it"
    );
    // 10: it is waiting again, on a question it has just asked. Not 20, because
    // nothing failed, and not 0.
    assert_eq!(
        run.code,
        Some(10),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );

    // **Two questions, naming two requests and two heads.** This is the assertion the
    // scenario is really about, and a bare "the pull request is still a draft" would
    // have been satisfied by a run that did nothing at all.
    let questions: Vec<(String, String)> = world
        .conversation()
        .iter()
        .filter(|comment| comment.is_bot)
        .map(|comment| {
            let binding = parse_marker(&comment.body).expect("a question carries its marker");
            (binding.request, binding.head_sha)
        })
        .collect();
    assert_eq!(
        questions,
        [
            (
                suspension.binding.request.clone(),
                suspension.binding.head_sha.clone()
            ),
            (questions[1].0.clone(), moved_to.clone()),
        ],
        "the first question stands, and a second names the head that now exists"
    );
    assert_ne!(
        questions[0].0, questions[1].0,
        "the two questions must name different requests: a run that re-asked under \
         the *same* request id would be the duplicate a continuation exists not to \
         create"
    );
    // And the approval a person already wrote answers neither of them as far as this
    // run is concerned: it was never interpreted, which the untouched model script
    // proves — the script holds one interpretation reply and the run consumed none,
    // so a walk that had reached step 7 would have found the gateway still holding
    // it and this assertion could not distinguish that. The `draft` and
    // `graphql_calls` assertions above are what carry the claim; this is the
    // conversation a person is left looking at.
    assert_eq!(
        world.posted_comment_bodies().len(),
        2,
        "one question from the suspension and one from this run: {:?}",
        world.posted_comment_bodies()
    );
}

/// The edited approval. A reply that was rewritten after it was listed is not the
/// reply that was listed, and the run refuses rather than acting on either version.
///
/// This is the one scenario in the file that turns on the two reads of the
/// conversation **disagreeing**, which is what step 5 exists for and what
/// [`World::edit_comment_on_next_read`] is the only way to say. The model is never
/// reached: step 5 precedes step 7, so the script carries no interpretation and a walk
/// that read this approval anyway fails at the socket.
#[test]
fn an_approval_edited_between_the_listing_and_the_re_read_is_refused() {
    let world = World::with_model_script(a_real_repair());
    let suspension = suspend(&world);
    let id = world.post_comment(AUTHORIZED, APPROVAL);
    world.edit_comment_on_next_read(id, "actually, no");
    world.accept_the_ready_mutation();

    let run = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        world.pull_request(suspension.pull_request)["draft"],
        serde_json::json!(true),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "neither version of an edited reply is one to act on"
    );
    // 11 and not 20: `DecisionError::ReplyEdited` is `Recurrence::Correctable`,
    // because an edit is a race a later walk re-reads past — the reply is whatever it
    // settles at, and this walk simply declines to act on a text that moved under it.
    assert_eq!(
        run.code,
        Some(11),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );
    // Neither version — said as an assertion rather than as a sentence. The listing
    // still carries the approval, so a walk that preferred the listing to the re-read
    // would have found a perfectly good approval to act on, and the count above is
    // what says it did not.
    assert_eq!(
        world
            .conversation()
            .iter()
            .filter(|comment| comment.id == id && comment.body == APPROVAL)
            .count(),
        1,
        "the listing must still offer the approval, or this refusal had nothing to \
         refuse: {:?}",
        world.conversation()
    );
}

/// An edited *request* is a record that is not the record.
///
/// # Two tamperings, because they are refused by two different rules
///
/// A reader would call both of these "somebody edited fiddle's question", and the
/// product tells them apart:
///
/// - **the marker rewritten, both reads agreeing.** Timestamps stay equal, so there is
///   no evidence of an edit at all — only a payload digest that disagrees with what
///   this run rebuilds. Refused at step 8 as `ForeignPayload`, **after** the model has
///   read the approval. That is the sharp case: the run held a genuine approval from
///   the nominated approver and refused anyway, on its own recomputation.
/// - **the question edited between the two reads.** Refused at step 5 as
///   `RequestEdited`, on the timestamp, without the rewritten bytes being weighed at
///   all — fiddle wrote that comment and has no path that edits one.
///
/// Neither is worked around. Nothing recomputes the marker from the conversation,
/// chooses between the two readings, or falls back to the earlier observation.
#[test]
fn an_edited_request_comment_is_refused_rather_than_recomputed_around() {
    // The marker rewritten, where both reads agree and only the arithmetic objects.
    let rewritten = World::with_model_script(a_suspension_and_its_approval(APPROVAL));
    let suspension = suspend(&rewritten);
    let question = rewritten.the_only_request_comment();
    let tampered = question.body.replace(
        &format!("payload={}", suspension.binding.payload),
        &format!("payload={}", "0".repeat(16)),
    );
    assert_ne!(
        tampered, question.body,
        "the rewrite must have changed the marker, or nothing is being tested"
    );
    rewritten.rewrite_the_published_question(&tampered);
    // Read back through the listing, so the tampering is a fact about the world this
    // run will read rather than about a string this test built.
    assert_eq!(
        parse_marker(&rewritten.the_only_request_comment().body)
            .expect("the tampered marker still parses; it is the digest that is wrong")
            .payload,
        "0".repeat(16),
        "the conversation must now carry the rewritten payload"
    );
    rewritten.post_comment(AUTHORIZED, APPROVAL);
    rewritten.accept_the_ready_mutation();

    let refused = rewritten.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        rewritten.pull_request(suspension.pull_request)["draft"],
        serde_json::json!(true),
        "a marker somebody rewrote is the one thing the design must not trust: \
         stdout={} stderr={}",
        refused.stdout,
        refused.stderr
    );
    assert_eq!(rewritten.graphql_calls(), 0);
    // 20: `ForeignPayload` is `Recurrence::Permanent`. Nothing a repeat does re-derives
    // a digest that agrees with a marker somebody rewrote, so a caller is told to stop
    // rather than to try again. **And the model was reached first** — step 8 follows
    // step 7 — so this run held an interpreted approval from the nominated approver
    // and refused on its own arithmetic anyway.
    assert_eq!(
        refused.code,
        Some(20),
        "stdout={} stderr={}",
        refused.stdout,
        refused.stderr
    );

    // The question edited between the two reads, refused on the timestamp instead.
    let edited = World::with_model_script(a_real_repair());
    let second = suspend(&edited);
    let question = edited.the_only_request_comment();
    edited.edit_comment_on_next_read(question.id, &question.body.replace("May", "Must"));
    edited.post_comment(AUTHORIZED, APPROVAL);
    edited.accept_the_ready_mutation();

    let refused = edited.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        edited.pull_request(second.pull_request)["draft"],
        serde_json::json!(true),
        "fiddle has no path that edits its own question, so an edited one is \
         somebody else's: stdout={} stderr={}",
        refused.stdout,
        refused.stderr
    );
    assert_eq!(edited.graphql_calls(), 0);
    // 20 as well, by a different route: `RequestEdited` is `Recurrence::Permanent`,
    // because the timestamps of an edited comment never return to agreeing. The two
    // tamperings above therefore share an exit code and share no mechanism, which is
    // exactly why neither would have been proved by the other.
    assert_eq!(
        refused.code,
        Some(20),
        "stdout={} stderr={}",
        refused.stdout,
        refused.stderr
    );
}

/// A second comment naming one request is a state to report, never a set to pick
/// from.
///
/// Reached through the binary because the shape a person could actually create —
/// copying fiddle's own comment out of the conversation and pasting it back — is a
/// **body**, not a struct. And it is posted as a *person*, which is what makes the
/// scenario sharp: the duplicate detection cannot be a filter on authorship, because
/// this copy is not a bot's.
///
/// # Which of the two duplicate guards fires, and how the exit code says so
///
/// There are two, and they are not redundant: `PublishDecisionRequest::inspect`
/// counts the comments carrying this run's marker before the continuation begins, and
/// step 2 of the validation order counts them again while choosing candidate replies.
/// One guards publishing a second question, the other guards reading a reply relative
/// to two of them.
///
/// **The exit code is what distinguishes them, and it is 11.** Step 2's
/// `DecisionError::DuplicateRequest` is `Recurrence::Permanent` — exit 20 — so a run
/// refusing there would say 20. Exit 11 says the refusal came from `inspect`'s
/// `GhError::Duplicate` instead, as `Correctable`: a person deleting the copy makes
/// the next attempt succeed. So this scenario pins the first guard a continuation
/// meets, and the code is the evidence for *which* rather than decoration on it.
#[test]
fn a_copied_request_comment_stops_the_run_rather_than_being_chosen_between() {
    let world = World::with_model_script(a_real_repair());
    let suspension = suspend(&world);
    let question = world.the_only_request_comment();
    let copy = world.post_comment(AUTHORIZED, &question.body);
    world.post_comment(AUTHORIZED, APPROVAL);
    world.accept_the_ready_mutation();

    // The denominator: two comments really do name the one request now, and the copy
    // is not a bot's. A world holding one would make the refusal below about
    // something else entirely.
    let naming = world.comments_naming(&suspension.binding.request);
    assert_eq!(
        naming.len(),
        2,
        "two comments must name the request: {naming:?}"
    );
    assert!(
        naming
            .iter()
            .any(|comment| comment.id == copy && !comment.is_bot),
        "the copy must be a person's, or this tests a bot filter instead: {naming:?}"
    );

    let run = world.fiddle([
        "run",
        "--capability",
        "propose_change",
        INVOCATION_REF,
        "--json",
    ]);
    assert_eq!(
        world.pull_request(suspension.pull_request)["draft"],
        serde_json::json!(true),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "there is no principled way to pick between two questions, so nothing is \
         spent on either"
    );
    // 11, and the number is load-bearing here: see this test's documentation for why
    // it is what says the operation's own `inspect` refused rather than step 2.
    assert_eq!(
        run.code,
        Some(11),
        "stdout={} stderr={}",
        run.stdout,
        run.stderr
    );
    // And no third comment was published to resolve the confusion.
    assert_eq!(
        world.comments_naming(&suspension.binding.request).len(),
        2,
        "the run must not have added a question of its own: {:?}",
        world.comments_naming(&suspension.binding.request)
    );
}
