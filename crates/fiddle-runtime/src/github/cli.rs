//! The one place a GitHub credential is turned into a running process.
//!
//! This module stands to the GitHub token exactly as [`crate::gateway`] stands
//! to the model key: one construction site, so "where could this credential
//! go?" has one answer. The environment below is the whole of what the child
//! sees.
//!
//! **The allowlist is five names, and this is the statement of it.** `PATH`,
//! inherited from this process or [`MINIMUM_PATH`] when it has none; `GH_TOKEN`,
//! the resolved credential; `GH_CONFIG_DIR`, pointed at a scratch directory so
//! the configuration source is pinned; `GH_PROMPT_DISABLED`, because a prompt in
//! an unattended run is a hang; and `NO_COLOR`, because ANSI escapes in output
//! that is going to be parsed are a defect waiting to be written.
//! `github_cli::the_gh_environment_is_exactly_five_names_and_no_home` asserts
//! that set exactly, against what the child actually received, so a sixth name
//! cannot arrive without an assertion changing.
//!
//! This is a *different* set from the four names a workspace check runs under
//! (`HOME`, `LANG`, `PATH`, `RUSTUP_HOME`), and the two are deliberately not
//! reconciled. They are different spawn sites with different needs, and the one
//! thing that would be genuinely wrong is widening the workspace's set to make
//! GitHub work. What the two share is the *bound* — the process group, the
//! deadline, the cancellation — which lives in [`crate::process`] and is
//! written once.
//!
//! # `HOME` is absent, and that is the load-bearing line
//!
//! With no `HOME` and a `GH_CONFIG_DIR` pointing at an empty directory, `gh`
//! answers "To get started with GitHub CLI, please run: gh auth login" rather
//! than reaching the operator's keyring. So "this adapter used the credential it
//! was given and no other" is a fact about the process rather than a promise in
//! a comment. Adding `HOME` back — even pointed at a scratch directory — would
//! reopen `~/.config/gh`, and the guarantee would quietly become a guarantee
//! about today's `gh`.
//!
//! # Why the status is parsed rather than inferred
//!
//! `gh help exit-codes` documents the whole set: **0** success, **1** any
//! failure, **2** cancelled, **4** authentication required. A 404, a 422 and a
//! 500 are all exit 1, so the HTTP status is simply not in the exit code. Every
//! call is therefore `gh api -i`, whose first line of stdout is the status line,
//! and [`GhCli::api`] reads the status from there. A branch that decided
//! anything about the response from exit 1 would have read the wrong surface.
//!
//! # Where the status line stops being the verdict
//!
//! That rule holds for every REST call and fails for exactly one call shape. A
//! refused GraphQL mutation answers **200** with a `null` field and an
//! `errors[]` beside it, so `status >= 400` reads it as a success —
//! `scripts/verify-graphql-ready.sh` measures the response and
//! [ADR 018](../../../docs/technical/decisions/018-a-graphql-200-is-not-a-success.md)
//! is the decision. Rather than make [`GhCli::api`]'s contract conditionally
//! untrue, GraphQL is a sibling: [`GhCli::graphql`] shares this module's
//! environment, bound, credential and status-line parse through
//! [`GhCli::command`] and [`GhCli::dispatch`], and differs in the verdict alone.
//! Nothing about the five names or the process group is about the URL, so
//! nothing about them changes.
//!
//! `gh` also has no timeout flag, so the runtime owns the deadline — which is
//! not only a cost. A `gh` killed after it has dispatched a request is a real
//! ambiguous write rather than a simulated one, and [`GhError::outcome`] is what
//! keeps it from being reported as a failure.
//!
//! # The two provenances of one interruption
//!
//! Three of this module's failures describe a child that was already running when
//! something ended it — the deadline, a signal, and a **cancellation** — and the
//! third is the one a `^C` produces, because [`crate::process`] puts every bounded
//! child in a process group of its own and the token is then the only channel that
//! reaches it. All three are `Unknown`.
//!
//! A cancellation *refused before any child exists* is the opposite fact and has
//! its own variant. Keeping the two apart is what
//! [`GhError::CancelledBeforeSpawn`] and [`GhError::CancelledAfterSpawn`] are for,
//! and merging them was a real defect this milestone shipped and had to repair:
//! one variant carried both, was classified for the harmless one, and documented
//! only that one.

use crate::effect::EffectOutcome;
use crate::git::GitError;
use crate::process::{run_bounded, Bounded};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Where to look for `gh` when this process was started without a `PATH`.
///
/// The same narrowing the workspace runner makes, for the same reason: a `PATH`
/// says *where a program is*, not *who the process may act as*, so inheriting
/// one grants the child nothing it could not reach by absolute path. The
/// credential is the thing that stays under this module's control.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

/// What `gh api -i` said, once the status line and the headers have been read
/// off the front of it.
#[derive(Debug)]
pub struct GhResponse {
    /// From the status line, never from the exit code.
    pub status: u16,
    /// The response body, parsed. `Null` when there was none — a 204 from a
    /// workflow dispatch is the ordinary case.
    pub body: serde_json::Value,
    /// `Retry-After`, when the response carried one.
    pub retry_after: Option<Duration>,
    /// `X-RateLimit-Remaining`, when the response carried one.
    pub rate_limit_remaining: Option<u64>,
    /// `Link`, when the response carried one.
    ///
    /// The third header this client reads, and the only one that is about the
    /// *answer* rather than about the credential. A listing says how much of
    /// itself it returned nowhere else: the page size is GitHub's choice, so a
    /// short page is not an end, and a client counting what came back cannot
    /// tell a complete conversation from the first hundredth of one.
    /// [`comments::read_conversation`](super::comments::read_conversation) is
    /// what reads it, and what it needs is `rel="next"`.
    ///
    /// **A length check is cheaper and is wrong**, which is worth stating here
    /// because the next reader will otherwise wonder why a header is carried
    /// for something a comparison against `per_page` appears to answer. The
    /// two disagree in both directions. A short page is not an end, so a client
    /// that stopped on one would miss every comment after it; and a page that
    /// happens to be exactly full is not a continuation, so a client that kept
    /// going on one would fetch a page nobody has to answer.
    /// `a_response_carries_actor_identity_and_edit_state_from_the_listing_alone`
    /// pins the second half of that: one scripted page, and exactly one request
    /// recorded.
    ///
    /// Carried whole and unparsed, because the relations are the reader's
    /// business and a header this module reduced to a boolean would be one
    /// every later listing had to widen again.
    pub link: Option<String>,
}

/// What a response said about being asked the same question again.
///
/// The two headers were parsed off `gh api -i` from the first day of this
/// milestone and read by nobody, which is the defect ADR 013 had to price for
/// `agent.max_capability_attempts`: a value that parses, defaults and reaches
/// nothing looks like a feature and behaves like a comment. This type is the
/// channel that carries them somewhere — out of [`GhResponse`], into
/// [`GhError::Http`], and from there into the postcondition read's wait.
///
/// It is carried on the *error* as well as on the response because that is
/// where it matters: a 429, or a secondary-rate-limit 403, is a response the
/// client is being told to ask for again later, and the client only ever sees
/// those as a [`GhError`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetryAdvice {
    /// `Retry-After`, when the response carried one. An instruction with a
    /// number attached, and the only wait in this system that GitHub chose
    /// rather than fiddle.
    pub retry_after: Option<Duration>,
    /// `X-RateLimit-Remaining`, when the response carried one.
    pub rate_limit_remaining: Option<u64>,
}

impl RetryAdvice {
    /// Whether the response asked to be left alone for a while.
    ///
    /// A `Retry-After` says so outright. A remaining budget of zero says the
    /// same thing without a number: the refusal is about the *credential's
    /// allowance*, which refills, rather than about the request, which does
    /// not. Both make a 403 that would otherwise be final into one worth
    /// looking past — and that distinction is this field's only job, so a
    /// deployment can tell "you may not" from "not just now".
    pub fn wants_a_wait(&self) -> bool {
        self.retry_after.is_some() || self.rate_limit_remaining == Some(0)
    }
}

/// How a `gh` invocation, or the push delegated beside it, can fail.
///
/// Not only `gh`: [`GhError::Push`] carries a [`crate::git::GitError`] so a push
/// failure crosses the executor boundary with git's own verdict intact, rather
/// than being given a fabricated HTTP status nobody sent.
///
/// The variants exist to be *classified*, not merely reported: see
/// [`GhError::outcome`], which is where each one commits to whether the request
/// it describes may have changed the world.
#[derive(Debug, thiserror::Error)]
pub enum GhError {
    /// Exit 4. `gh` had no usable credential, so nothing was sent.
    #[error("gh could not authenticate (exit 4)")]
    Auth,
    /// A cancellation this runtime raised **before any child existed**, at the
    /// check in [`GhCli::dispatch`] — the one and only producer, and shared by
    /// every call shape because it is the spawn rather than the request that it
    /// is about.
    ///
    /// This is the only cancellation that is *knowledge*: no process was started,
    /// so no request left this one, so the world is untouched. It is therefore the
    /// only one classified `NotCommitted`.
    ///
    /// Its sibling is [`GhError::CancelledAfterSpawn`], and the two must never be
    /// merged back together. M2's holistic review found them merged: one variant
    /// carried both provenances, was classified `NotCommitted` for the harmless
    /// one, and its own documentation described only that one. A `^C` landing
    /// during `POST .../pulls` was reported as a write that had definitely not
    /// happened.
    #[error("gh was cancelled before it was started")]
    CancelledBeforeSpawn,
    /// A cancellation that reached a `gh` which was **already running**. Two
    /// producers, and they are the whole of the variant:
    ///
    /// 1. the run's token winning [`crate::process::run_bounded`]'s `select!`,
    ///    which kills the child's process group — the same `reap` on the same
    ///    already-spawned child that produces [`GhError::Timeout`];
    /// 2. exit **2**, `gh`'s own spelling of "cancelled", from a child that ran
    ///    and answered nothing.
    ///
    /// Both are ambiguous writes and both are classified `Unknown`, for exactly
    /// the reason [`GhError::Killed`] is: the request may already be at GitHub and
    /// it is the *reply* that was lost. A cancellation is also the only one of
    /// this group a `^C` can produce — [`crate::process`] gives every bounded
    /// child a process group of its own, so a terminal interrupt reaches it only
    /// through the token — which makes this the reachable half of the pair rather
    /// than the exotic one.
    #[error("gh was cancelled after it had already been started")]
    CancelledAfterSpawn,
    /// The runtime's own deadline passed and the process group was killed.
    #[error("gh exceeded its {0:?} timeout and was killed")]
    Timeout(Duration),
    /// The child died without answering — a signal, or an exit code above 128.
    /// Distinct from [`GhError::Timeout`] because the runtime did not choose it,
    /// and classified `Unknown` for the same reason: the request may have
    /// landed.
    #[error("gh was killed before it answered (status {0})")]
    Killed(String),
    /// A request this runtime **refused to send**, having decided the call itself
    /// was wrong.
    ///
    /// One producer today:
    /// [`EnsureCheckRequested::apply`](crate::github::EnsureCheckRequested)'s
    /// identity guard, which refuses a dispatch that would be *named* by one
    /// identity and *looked up* by another. `NotCommitted` is the whole point of
    /// the variant — nothing was sent, so no postcondition read is owed — and it
    /// is separate from [`GhError::Malformed`] because that neighbour is now
    /// `Unknown`: a guard that refuses before dispatching and a `gh` whose answer
    /// could not be read are opposite facts about the world, and one variant
    /// cannot carry both.
    #[error("nothing was sent: {0}")]
    NotSent(String),
    /// A response arrived and carried a status at or above 400.
    ///
    /// `advice` is what that response said about being asked again. It is not
    /// in the [`std::fmt::Display`] rendering on purpose: the message is what
    /// reaches an operator and a published bundle, and "wait two seconds" is an
    /// instruction to the client rather than a description of the failure.
    #[error("HTTP {status}: {message}")]
    Http {
        status: u16,
        message: String,
        advice: RetryAdvice,
    },
    /// **A 200 from GraphQL that was not a success.**
    ///
    /// Two producers, and the second is why this is not named for a refusal:
    ///
    /// 1. a request GitHub **refused while answering 200**, which is the ordinary
    ///    case and the one the fields are named for;
    /// 2. a 200 [`GhCli::graphql`] **could not interpret at all** — a body that is
    ///    not an object, so it carried neither `data` nor `errors`. `kind` is
    ///    `"UNKNOWN"` there, which is the same answer this variant already gives a
    ///    refusal whose type it could not read, and lands in the same `Unknown`
    ///    arm for the same reason.
    ///
    /// The status line says nothing here, and neither does the exit code: a
    /// refused mutation is `HTTP/2.0 200` with `data.<field>` null and an
    /// `errors[]` carrying the verdict, and `gh` exits 1 as it does for every
    /// other failure. `kind` is `errors[0].type` — the machine-readable half,
    /// which is what makes a classification possible at all — or `"UNKNOWN"`
    /// when the response carried no type this client could read. `message` is
    /// its human half, redacted like every other diagnostic.
    ///
    /// [`GhError::outcome`] holds the classification and
    /// [ADR 018](../../../docs/technical/decisions/018-a-graphql-200-is-not-a-success.md)
    /// its reasons. This variant exists rather than a reused [`GhError::Http`]
    /// with a fabricated status because there is no status to fabricate that
    /// would not be a number nobody sent — the same argument [`GhError::Push`]
    /// makes for git's own verdict.
    #[error("GraphQL {kind}: {message}")]
    GraphQl { kind: String, message: String },
    /// **No readable answer came back from a child that may well have run.**
    ///
    /// Four producers, and naming all four is what the classification rests on:
    ///
    /// 1. [`crate::process::run_bounded`] returning an `io::Error` — either the
    ///    spawn failing or the wait failing. Those two are *not* distinguishable
    ///    at that call site, and the pair is classified together deliberately:
    ///    the spawn half reached nothing, the wait half is a child this process
    ///    lost hold of, and the conservative reading of the pair costs one `GET`
    ///    while the confident one costs a duplicate. The same trade
    ///    [`GitError::Push`](crate::git::GitError::Push) already makes.
    /// 2. `stdout` carrying no `HTTP/` status line at all — `cli.program`
    ///    pointing at something that is not `gh`, a wrapper that printed a
    ///    warning first, a transport error on `stderr`.
    /// 3. A status line whose second token is not a number, or a non-empty body
    ///    that is not JSON.
    /// 4. A response that parsed and did not carry the fields this client needs
    ///    (`crate::github::pulls`, `crate::github::checks`, `crate::github::refs`).
    ///
    /// **This is `Unknown`, and it used to be `NotCommitted`.** The old reading
    /// was that a process which ran to a normal completion and produced garbage
    /// is a broken runner rather than an ambiguous write. That is true of the
    /// *runner* and false of the *world*: a wrapper is free to deliver the
    /// request and mangle what it prints afterwards, and §6.5's rule is that a
    /// lost response is not evidence of a failed write. What stays true is that a
    /// program which is not `gh` will not become one, so this is `Unknown`
    /// *without* being worth reading again — see
    /// [`GhError::is_worth_reading_again`].
    #[error("gh output could not be parsed: {0}")]
    Malformed(String),
    /// More objects matched than the postcondition allows. Reported, never
    /// resolved by picking the first.
    #[error("{count} objects matched where at most one was expected")]
    Duplicate { count: usize },
    /// A push, which is `git` rather than `gh`.
    ///
    /// [`IntegrationOperation`](crate::effect::IntegrationOperation) fixes one
    /// error type for every operation's mutation, and `ensure_branch_published`
    /// is the one whose mutation is not an API call: a ref can only point at an
    /// object the remote already has, so the objects and the ref are published
    /// together by [`GitCli::publish`](crate::git::GitCli::publish) and there is
    /// no `POST /git/refs` to fail instead.
    ///
    /// The [`GitError`] is carried whole rather than flattened into a message,
    /// and that is the point of the variant. [`GhError::outcome`] delegates to
    /// [`GitError::outcome`], so the judgment about whether a failed push may
    /// have moved the ref is made by the type that knows git's refusal channel —
    /// rather than by mapping a push failure onto an HTTP status nobody sent.
    #[error("the branch could not be pushed: {0}")]
    Push(#[from] GitError),
}

impl GhError {
    /// What this failure says about whether the request changed anything.
    ///
    /// A lost answer is `Unknown`; an explicit refusal is `NotCommitted`. The
    /// milestone turns on getting these two apart, because `Unknown` sends the
    /// caller to read the world and `NotCommitted` sends it to retry — and a
    /// landed write reported as `NotCommitted` is retried into a duplicate.
    ///
    /// **The test is "could this request have reached GitHub?", never "did this
    /// runtime work properly?".** Those two questions have different answers for
    /// a cancellation and for a garbled response, and answering the second one is
    /// how M2's holistic review found a `^C` during `POST .../pulls` reported as a
    /// write that had definitely not happened.
    pub fn outcome(&self) -> EffectOutcome {
        match self {
            // The answer was lost. The write may or may not have landed.
            //
            // `Killed` is the state the exactly-once harness deliberately
            // creates: the scripted `gh` applies its mutation and *then* exits
            // as though killed. `CancelledAfterSpawn` is its sibling and the
            // reachable one — `main`'s `^C` handler cancels the token, and the
            // token is the only channel that reaches a child in its own process
            // group. Both come out of a child that had already started, so both
            // say the same thing about the world as the deadline does.
            GhError::Timeout(_) | GhError::Killed(_) | GhError::CancelledAfterSpawn => {
                EffectOutcome::Unknown
            }
            // GitHub failed after receiving the request. Whether it got far
            // enough to act is not something a 5xx tells anyone.
            GhError::Http { status, .. } if *status >= 500 => EffectOutcome::Unknown,
            // 422 covers malformed input, invalid ref syntax, spam protection
            // and "already exists" — a refusal and a success wearing the same
            // number. It is never classified on its face; being `Unknown` is
            // what forces the caller into the postcondition read that can
            // actually tell those apart.
            GhError::Http { status: 422, .. } => EffectOutcome::Unknown,
            // Every other 4xx is GitHub saying it declined, in terms that leave
            // no room for it having acted anyway.
            GhError::Http { .. } => EffectOutcome::NotCommitted,
            // The same two questions asked of a refusal that arrived as a 200.
            // `NOT_FOUND` and `FORBIDDEN` are conclusions *about the request* —
            // there was no node to reach, or GitHub declined before acting — so
            // they are the GraphQL spelling of the 4xx arm above.
            //
            // Everything else is `Unknown`, and the default is the load-bearing
            // half rather than a fallthrough. `UNPROCESSABLE` is REST 422's own
            // reason in another spelling: the probe issued one cause — creating
            // a ref that already exists — down both surfaces and got a 422 from
            // one and an `UNPROCESSABLE` from the other, so it inherits 422's
            // ambiguity along with its number. An unrecognised or absent type is
            // `Unknown` for a different reason: GitHub's error-type set is
            // GitHub's to extend, and `NotCommitted` is the claim that permits a
            // retry, so a name this build has never seen must cost a second read
            // rather than a second write. The 200 that could not be interpreted at
            // all arrives as `UNKNOWN` and is carried by that same default, which
            // is the third thing it is load-bearing for.
            GhError::GraphQl { kind, .. } => match kind.as_str() {
                "NOT_FOUND" | "FORBIDDEN" => EffectOutcome::NotCommitted,
                _ => EffectOutcome::Unknown,
            },
            // Two objects where one was expected means an earlier write is
            // unaccounted for; that is not a settled world.
            GhError::Duplicate { .. } => EffectOutcome::Unknown,
            // No answer came back that this client could read. `Unknown` and not
            // `NotCommitted`, because "the runner is broken" is a fact about the
            // runner: a wrapper is free to deliver the request and then mangle
            // what it prints, and §6.5's rule is that a lost response is not
            // evidence of a failed write. See the variant for all four of its
            // producers, including the spawn failure that certainly reached
            // nothing and is classified here anyway.
            GhError::Malformed(_) => EffectOutcome::Unknown,
            // Nothing left this process. `Auth` is `gh` refusing before it
            // dispatches; `CancelledBeforeSpawn` is this runtime refusing before
            // any child exists; `NotSent` is this runtime refusing a call it had
            // decided was wrong. Each is an absence of a request rather than an
            // absence of an answer, which is the whole distinction this function
            // is for.
            GhError::Auth | GhError::CancelledBeforeSpawn | GhError::NotSent(_) => {
                EffectOutcome::NotCommitted
            }
            // Delegated rather than restated: git's refusal channel is the
            // porcelain report, and only `GitError` knows which of its variants
            // came from one.
            GhError::Push(error) => error.outcome(),
        }
    }

    /// What the response said about being asked again, if it said anything.
    ///
    /// Total over the variants rather than a match at each call site: every
    /// failure has to answer "how long should I wait", and the ones that never
    /// carried a header answer it with [`RetryAdvice::default`] — which is not
    /// silence, it is "nothing was said", and the backoff supplies its own
    /// number for that case.
    pub fn advice(&self) -> RetryAdvice {
        match self {
            GhError::Http { advice, .. } => *advice,
            _ => RetryAdvice::default(),
        }
    }

    /// Whether asking the same question again could plausibly answer it
    /// differently.
    ///
    /// **This is asked only of a read.** Nothing consults it about a mutation,
    /// and the reason is the rule the whole milestone rests on: a read is
    /// idempotent, so looking again costs nothing, while re-dispatching a write
    /// whose answer was lost is how a duplicate external effect is born. See
    /// [`Executor::execute`](crate::effect::Executor::execute).
    pub fn is_worth_reading_again(&self) -> bool {
        match self {
            // The answer was lost on the way back. The next one may arrive.
            //
            // `CancelledAfterSpawn` belongs here for the same reason as the other
            // two, and belongs here *in the type* even where the caller stops
            // sooner: `read_until_settled` selects on the run's token, so a
            // cancelled run takes its one observation and leaves rather than
            // waiting out a backoff. This answers what the error means, not how
            // long the caller may spend on it.
            GhError::Timeout(_) | GhError::Killed(_) | GhError::CancelledAfterSpawn => true,
            // GitHub said so itself — either with a `Retry-After` or by
            // reporting the credential's allowance spent. This arm is ahead of
            // the status arms because it is the one that separates a
            // secondary-rate-limit 403, which passes, from a permissions 403,
            // which does not.
            GhError::Http { advice, .. } if advice.wants_a_wait() => true,
            // 429 is the rate limit saying so without a header; 5xx is GitHub
            // failing at something that is not about this request.
            GhError::Http { status, .. } => *status == 429 || *status >= 500,
            // Derived from the classification rather than restated beside it, so
            // the two cannot drift: a `NOT_FOUND` is settled and will still be
            // not found, while the kinds this build could not settle are exactly
            // the ones another look might. `Unknown` does not imply this in
            // general — `Malformed` is the counterexample — but within this
            // variant every `Unknown` is an unsettled *answer* rather than a
            // broken runner.
            GhError::GraphQl { .. } => self.outcome() == EffectOutcome::Unknown,
            // Everything else is settled. A refusal stays refused; a request that
            // was never started has nothing to look for; a program that is not
            // `gh` will not become one however often it is asked, which is why
            // `Malformed` is `Unknown` and still not worth another read; and a
            // second matching object does not become one object by being looked
            // at again.
            //
            // `Push` is `false` for a different reason than the rest, and it is
            // worth naming rather than leaving to the reader: this question is
            // asked **only of a read**, and a read is never a push. Delegating to
            // `GitError` here would be a branch nothing can reach.
            GhError::Auth
            | GhError::CancelledBeforeSpawn
            | GhError::NotSent(_)
            | GhError::Malformed(_)
            | GhError::Duplicate { .. }
            | GhError::Push(_) => false,
        }
    }
}

/// A `gh` that carries one credential and runs under one environment.
///
/// `program` and `args` are the operator seam — `[github] cli = { program,
/// args }` exists because someone may have to pin a `gh` version or put a
/// wrapper in front of it, and it is the same seam `[workspace] check` already
/// offers. The deterministic suite substitutes a scripted `gh` there; nothing
/// fake enters the product to make that possible.
pub struct GhCli {
    program: PathBuf,
    args: Vec<String>,
    /// The resolved credential, held as a `String` and passed to one child's
    /// environment — the same shape [`crate::gateway`] uses for the model key,
    /// rather than a wrapper type this workspace does not have.
    token: String,
    /// What diagnostics name. An error can say which variable was empty without
    /// ever rendering what was in it, which is the whole reason the name is
    /// carried separately from the value.
    variable: String,
    config_dir: PathBuf,
    timeout: Duration,
}

/// Hand-written rather than derived, because a derived one would print
/// `token`.
///
/// This is not paranoia about a field nobody prints: `{:?}` on a struct is what
/// a `dbg!`, an `unwrap` on a `Result<_, _>` containing one, or a tracing
/// attribute reaches for by default, and M1 shipped a defect in exactly this
/// class — a response body that echoed the received key reached a published
/// bundle. The variable's *name* is here because that is the actionable half.
impl std::fmt::Debug for GhCli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GhCli")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("credential_from", &self.variable)
            .field("config_dir", &self.config_dir)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl GhCli {
    /// Build the one `gh` this process will run.
    ///
    /// `token` is taken by value and `variable` names where it came from, the
    /// same division [`crate::gateway::completion_model`] makes: the caller owns
    /// resolving the credential because it owns the configuration, and this
    /// module owns everything that happens to it afterwards.
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        token: String,
        variable: &str,
        config_dir: PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            program,
            args,
            token,
            variable: variable.to_string(),
            config_dir,
            timeout,
        }
    }

    /// Which variable the credential came out of. A diagnostic may name this;
    /// nothing may name its value.
    ///
    /// **No production diagnostic names it yet.** The CLI reports a missing
    /// credential from the configuration side, before a [`GhCli`] exists at all,
    /// and the client's own errors quote `gh` rather than where the token came
    /// from. What reads this is `github_cli`, which pins the name the client was
    /// built with — and that assertion is the point of keeping it: it makes "the
    /// credential's source is configuration, not a name hardcoded in this module"
    /// checkable rather than asserted in prose. `GitCli` has no such reader and
    /// therefore no such accessor; its `Debug` carries the name instead.
    pub fn variable(&self) -> &str {
        &self.variable
    }

    /// One `gh api -i` call.
    ///
    /// `body`, when present, is written to the child's stdin behind `--input -`
    /// rather than passed as an argument. That is not only tidiness: `argv` is
    /// world-readable through `/proc/<pid>/cmdline` on Linux, and a request body
    /// is the kind of thing that grows a credential-shaped field later without
    /// anybody revisiting how it is passed.
    pub async fn api(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
        cancel: &CancellationToken,
    ) -> Result<GhResponse, GhError> {
        let mut command = self.command();
        command.arg("--method").arg(method).arg(path);
        let stdin = body.map(|body| {
            command.arg("--input").arg("-");
            body.to_string().into_bytes()
        });
        self.dispatch(&mut command, stdin, cancel).await
    }

    /// One `gh api graphql` mutation or query, whose verdict is in the body.
    ///
    /// A sibling of [`GhCli::api`] rather than a widening of it, for the reason
    /// [ADR 018](../../../docs/technical/decisions/018-a-graphql-200-is-not-a-success.md)
    /// gives: a refused mutation answers 200, so the two cannot share a verdict
    /// rule, and making `api`'s contract depend on which URL it was handed would
    /// turn each of the five REST operations into a call site to re-read. They
    /// share everything else — [`GhCli::command`] builds the same environment
    /// and [`GhCli::dispatch`] runs it under the same bound and reads the same
    /// status line.
    ///
    /// **Variables are variables and never text.** Each pair goes out as its own
    /// `-f name=value`, which `gh` binds as a GraphQL variable rather than as a
    /// form field — measured by step 0 of `scripts/verify-graphql-ready.sh`,
    /// because a mutation whose variables silently did not bind would fail in a
    /// way that looks like a permissions problem. Interpolating a value into the
    /// query text instead would let a node id carrying a quote rewrite the query
    /// it appears in, and a node id is a value this process passes on rather than
    /// one it chose.
    ///
    /// What comes back on success is `data`, and it is a *claim*: nothing here
    /// makes the mutation's own answer authoritative, and the executor's step 8
    /// reads the postcondition exactly as it does for every other operation.
    ///
    /// **A 200 this method cannot interpret is `Unknown`, not a success.** A body
    /// that is not an object — `null`, an empty response, an array, a string, a
    /// number — did not say what happened, and that is the same situation as an
    /// `errors` field of the wrong shape, which `refusal` already costs a read
    /// for. It used to be the one place this classification erred toward
    /// believing an outcome: `response.body["data"]` is `Null` for every one of
    /// those bodies, so each returned `Ok(Null)` and read as a mutation that
    /// succeeded and had nothing to report.
    ///
    /// ADR 018's direction-of-error argument decides it. An uninterpretable
    /// answer is not evidence that the mutation did not happen — it is the lost
    /// answer §6.5 is about — so the caller is sent to *read the world*: being
    /// wrong this way costs one postcondition read, and being wrong the other way
    /// costs a duplicate external effect.
    pub async fn graphql(
        &self,
        query: &str,
        variables: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, GhError> {
        let mut command = self.command();
        command
            .arg("graphql")
            .arg("-f")
            .arg(format!("query={query}"));
        for (name, value) in variables {
            command.arg("-f").arg(format!("{name}={value}"));
        }

        // A status at or above 400 is still a transport failure and still
        // `GhError::Http`, decided by the shared parse: nothing about GraphQL
        // changes what a 502 means. Only the 2xx needs a second look.
        let response = self.dispatch(&mut command, None, cancel).await?;

        // Read before `refusal`, because a body that is not an object has no
        // `errors` field to be shaped wrongly and would otherwise fall through as
        // a success. `GraphQl` with an unreadable kind rather than `Malformed`:
        // both are `Unknown`, and only this one is worth reading again, which is
        // the half that matters. `Malformed`'s `false` rests on "a program that is
        // not `gh` will not become one" — and this `gh` answered, with a readable
        // status line and a body that parsed, so the next answer to the same
        // question may well be interpretable.
        if !response.body.is_object() {
            return Err(GhError::GraphQl {
                kind: UNKNOWN_ERROR_TYPE.to_string(),
                message: self.redact(&format!(
                    "the response body was {} rather than an object, so it said \
                     neither data nor errors",
                    shape(&response.body)
                )),
            });
        }

        match refusal(&response.body) {
            Some((kind, message)) => Err(GhError::GraphQl {
                kind,
                message: self.redact(&message),
            }),
            None => Ok(response.body["data"].clone()),
        }
    }

    /// The one `gh` this module builds: an empty environment, the five names it
    /// is allowed, the operator's own arguments, and `api -i`.
    ///
    /// Written once and called by both call shapes, so "what does the child
    /// see?" has one answer whatever was asked. `-i` is here rather than at each
    /// caller because it is what makes the status line readable at all, and a
    /// call that omitted it would be a call whose failures could only be guessed
    /// at from an exit code that reports 404, 422 and 500 alike.
    fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command.env_clear();
        // A locator may be inherited, an authority may not — M1's rule, applied
        // at a second spawn site.
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        command.env("GH_TOKEN", &self.token);
        command.env("GH_CONFIG_DIR", &self.config_dir);
        command.env("GH_PROMPT_DISABLED", "1");
        command.env("NO_COLOR", "1");
        // Nothing else. A sixth entry here is a change to the security boundary
        // and has to break `the_gh_environment_is_exactly_five_names_and_no_home`
        // before it can land.

        command.args(&self.args);
        command.arg("api").arg("-i");
        command
    }

    /// Run a built `gh` under the runtime's bound and read what it answered.
    ///
    /// The other half of the single spawn site. Every call shape reaches GitHub
    /// through this function, so the cancellation check, the deadline, the
    /// process group and the status-line parse are one implementation rather
    /// than one per URL.
    async fn dispatch(
        &self,
        command: &mut tokio::process::Command,
        stdin: Option<Vec<u8>>,
        cancel: &CancellationToken,
    ) -> Result<GhResponse, GhError> {
        // Checked before spawning, not only raced against: cancellation has to
        // prevent the effect, and this is the one moment where refusing is free.
        //
        // It is also the one moment where a cancellation is *knowledge* — no child
        // exists, so no request left this process — which is why this returns a
        // different variant from the `select!` arm below. The two used to share
        // one, and the sharing is the defect M2's holistic review found.
        if cancel.is_cancelled() {
            return Err(GhError::CancelledBeforeSpawn);
        }

        // The deadline is the runtime's because `gh` has no flag for one. Own
        // process group and cancellation come with it — see [`crate::process`].
        let bounded = run_bounded(command, stdin, self.timeout, cancel)
            .await
            .map_err(|source| {
                GhError::Malformed(self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )))
            })?;

        match bounded {
            // The child was running when the token cancelled, so this request may
            // be at GitHub already — the same thing the deadline arm beside it
            // says, produced by the same kill on the same child.
            Bounded::CancelledAfterSpawn => Err(GhError::CancelledAfterSpawn),
            Bounded::TimedOut => Err(GhError::Timeout(self.timeout)),
            Bounded::Finished(output) => self.parse(&output),
        }
    }

    /// Turn a finished `gh` into a response or into a classified failure.
    ///
    /// The order of the arms is the argument. The exit code is consulted only
    /// for the three things it actually reports — authentication, cancellation,
    /// and the child having died rather than answered — and the HTTP status is
    /// read from the status line for everything else. Exit **1** deliberately
    /// falls through to the status line, because that single code covers a 404,
    /// a 422 and a 500 alike.
    fn parse(&self, output: &std::process::Output) -> Result<GhResponse, GhError> {
        match output.status.code() {
            Some(0) => {}
            // Exit 2 is `gh`'s own "cancelled", and it comes from a child that
            // *ran*. It therefore joins the after-spawn provenance rather than the
            // pre-spawn one: this process started a `gh`, that `gh` reached
            // whatever it reached, and then it reported a cancellation instead of
            // an answer. Reading it as "nothing was sent" would be reading the
            // exit code as evidence about GitHub, which is the mistake the whole
            // of this module's status-line parsing exists to avoid.
            Some(2) => return Err(GhError::CancelledAfterSpawn),
            Some(4) => return Err(GhError::Auth),
            // Nobody chose this one. `code()` is `None` when a signal ended the
            // process; a code at or above 128 is the shell's spelling of the
            // same thing, and a `gh` wrapper is exactly the sort of thing that
            // reports a killed child that way. Both reach `Killed`, and through
            // it `Unknown`, because a child that died on the way back tells us
            // nothing about whether the request landed.
            None => return Err(GhError::Killed("signal".to_string())),
            Some(code) if code >= 128 => return Err(GhError::Killed(code.to_string())),
            // Exit 1 and any other code: the response, if there is one, says
            // what happened.
            Some(_) => {}
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let (head, body) = match text.split_once("\r\n\r\n") {
            Some(split) => split,
            // A `gh` that normalized its line endings is still answering the
            // question; refusing to read it would turn a cosmetic difference
            // into an unresolved outcome.
            None => text.split_once("\n\n").unwrap_or((text.as_ref(), "")),
        };

        let mut lines = head.lines();
        let status_line = lines.next().unwrap_or_default();
        // Checked rather than assumed: without this, a `gh` that printed a
        // warning first would have its second token parsed as a status, and the
        // adapter would report a number nobody sent.
        if !status_line.starts_with("HTTP/") {
            // `stderr` is quoted only here, and only because this is the one
            // failure an operator cannot diagnose without it: when `program` is
            // not the `gh` it was configured to be, stdout is usually empty and
            // the reason is on the other stream. It is redacted and bounded like
            // everything else that can reach a log.
            return Err(GhError::Malformed(self.redact(&format!(
                "no HTTP status line in {} (stderr: {})",
                snippet(&text),
                snippet(&String::from_utf8_lossy(&output.stderr)),
            ))));
        }
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| {
                GhError::Malformed(
                    self.redact(&format!("unreadable status line {}", snippet(status_line))),
                )
            })?;

        let mut retry_after = None;
        let mut rate_limit_remaining = None;
        let mut link = None;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            // Header names are case-insensitive and HTTP/2 lowercases them,
            // so matching the spelling GitHub's documentation uses would work
            // over HTTP/1.1 and silently stop working over HTTP/2.
            match name.trim().to_ascii_lowercase().as_str() {
                "retry-after" => retry_after = value.parse().ok().map(Duration::from_secs),
                "x-ratelimit-remaining" => rate_limit_remaining = value.parse().ok(),
                "link" => link = Some(value.to_string()),
                _ => {}
            }
        }

        let body = parse_body(body).map_err(|reason| GhError::Malformed(self.redact(&reason)))?;

        // Assembled before the status is judged, so that both exits carry the
        // same parsed headers. The failure exit reads them straight back off
        // this value — which is the whole point: until this line the two header
        // fields were parsed and dropped, and a response a client is told to
        // retry is precisely the one that never reached the `Ok` arm where they
        // lived.
        let response = GhResponse {
            status,
            body,
            retry_after,
            rate_limit_remaining,
            link,
        };

        if response.status >= 400 {
            return Err(GhError::Http {
                status: response.status,
                // GitHub's error envelope, when there is one. The whole body is
                // deliberately not carried: it is the surface that reaches a
                // published bundle, and M1 already shipped one defect of that
                // shape.
                message: self.redact(
                    response.body["message"]
                        .as_str()
                        .unwrap_or("no message in the response body"),
                ),
                advice: RetryAdvice {
                    retry_after: response.retry_after,
                    rate_limit_remaining: response.rate_limit_remaining,
                },
            });
        }

        Ok(response)
    }

    /// Remove the credential from anything about to become a diagnostic.
    ///
    /// Belt and braces: nothing in this module puts the token into a message on
    /// purpose, and this is what makes that true of the messages it did not
    /// write — a response body, a spawn error naming an environment. The failure
    /// this guards against is not hypothetical; M1 published a gateway response
    /// body that echoed the key it had received.
    fn redact(&self, text: &str) -> String {
        match self.token.is_empty() {
            true => text.to_string(),
            false => text.replace(&self.token, "[redacted]"),
        }
    }
}

/// The type this client uses when a GraphQL error carried none it could read.
///
/// Not a spelling GitHub sends. It exists so that the classification has
/// something to match on and lands in the `Unknown` arm, which is where every
/// unrecognised refusal belongs.
const UNKNOWN_ERROR_TYPE: &str = "UNKNOWN";

/// Whether a 200 was actually a refusal, and what it said if so.
///
/// The whole of the verdict rule that separates [`GhCli::graphql`] from
/// [`GhCli::api`]. Two shapes are not a refusal, and everything else is:
///
/// - no `errors` at all — the ordinary success;
/// - an `errors` that is an empty array — GitHub does not send one, and a
///   classifier that read it as a refusal would fail every success from a server
///   that did.
///
/// An `errors` that is present and not an array is therefore a refusal typed
/// `UNKNOWN`, not a success. It is a body this client cannot read, and reading
/// it as a success would be believing an outcome on no evidence; typed unknown
/// it costs a second read instead, which is the direction every mistake in this
/// classification is meant to fall.
///
/// Only `errors[0]` is consulted. GitHub may send several, and taking the first
/// is what the classification is defined against; carrying all of them would
/// mean deciding which one the outcome comes from, which is the same choice made
/// less legibly.
fn refusal(body: &serde_json::Value) -> Option<(String, String)> {
    let errors = match &body["errors"] {
        serde_json::Value::Null => return None,
        serde_json::Value::Array(errors) if errors.is_empty() => return None,
        serde_json::Value::Array(errors) => errors,
        _ => {
            return Some((
                UNKNOWN_ERROR_TYPE.to_string(),
                "the response carried an errors field that is not an array".to_string(),
            ))
        }
    };

    let first = &errors[0];
    Some((
        first["type"]
            .as_str()
            .unwrap_or(UNKNOWN_ERROR_TYPE)
            .to_string(),
        first["message"]
            .as_str()
            .unwrap_or("no message in the error")
            .to_string(),
    ))
}

/// What a body was, for a diagnostic that must not quote it.
///
/// The shape and never the content: an operator needs to know *how* the answer
/// was unreadable, and the answer itself is the one thing that has been known to
/// carry a credential back — see [`GhCli::redact`]. `Null` covers the empty
/// response as well as a literal `null`, because [`parse_body`] reads a body that
/// is not there as `Null` and by this point the two are one case.
fn shape(body: &serde_json::Value) -> &'static str {
    match body {
        serde_json::Value::Null => "empty or null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// A response body, or `Null` when there was none.
///
/// An empty body is ordinary — `POST .../dispatches` answers 204 with nothing —
/// so it is not a parse failure. A non-empty body that is not JSON is: this
/// client only ever asks for JSON, so anything else means the thing on the far
/// end of `program` is not the `gh` it was configured to be.
fn parse_body(body: &str) -> Result<serde_json::Value, String> {
    let body = body.trim();
    if body.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(body).map_err(|error| format!("body is not JSON ({error})"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    /// A finished `gh`, as the parser sees one. Exit 1 rather than 0, because
    /// that is what `gh` really exits with for every HTTP failure and the
    /// status must come off the status line regardless.
    fn answered(head: &str, body: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: format!("{head}\r\n\r\n{body}").into_bytes(),
            stderr: Vec::new(),
        }
    }

    fn client() -> GhCli {
        GhCli::new(
            PathBuf::from("/nonexistent/gh"),
            Vec::new(),
            String::new(),
            "GH_TOKEN",
            PathBuf::from("/nonexistent"),
            Duration::from_secs(1),
        )
    }

    /// The two headers were parsed from the first day of this milestone and
    /// read by nobody. This is the half of their first consumer that can be
    /// asserted against a real response: a rate-limited read is precisely the
    /// one that never reaches the `Ok` arm the fields lived on, so a client
    /// that only carried them on success carried them nowhere.
    #[test]
    fn a_rate_limited_response_carries_its_headers_into_the_error() {
        let error = client()
            .parse(&answered(
                "HTTP/2.0 429 Too Many Requests\r\n\
                 Retry-After: 2\r\n\
                 X-RateLimit-Remaining: 0",
                r#"{"message":"API rate limit exceeded"}"#,
            ))
            .expect_err("a 429 is a failure");

        assert_eq!(
            error.advice(),
            RetryAdvice {
                retry_after: Some(Duration::from_secs(2)),
                rate_limit_remaining: Some(0),
            },
            "got {error:?}"
        );
        assert!(
            error.is_worth_reading_again(),
            "and the advice must be what makes it worth another look"
        );
    }

    /// A refusal that said nothing about waiting says nothing about waiting.
    ///
    /// The contrast is the point: without it, an implementation that reported
    /// every failure as retryable would pass the case above.
    #[test]
    fn a_refusal_with_no_headers_advises_nothing() {
        let error = client()
            .parse(&answered(
                "HTTP/2.0 403 Forbidden",
                r#"{"message":"Resource not accessible by integration"}"#,
            ))
            .expect_err("a 403 is a failure");

        assert_eq!(error.advice(), RetryAdvice::default(), "got {error:?}");
        assert!(!error.is_worth_reading_again());
    }

    /// The headers still reach a successful response, unchanged. `Retry-After`
    /// on a 2xx is GitHub asking a client to come back for an answer it is
    /// still preparing, and dropping it here would move the defect rather than
    /// fix it.
    #[test]
    fn a_successful_response_still_carries_its_headers() {
        let response = client()
            .parse(&answered(
                "HTTP/2.0 202 Accepted\r\nRetry-After: 5\r\nX-RateLimit-Remaining: 4999",
                "",
            ))
            .expect("a 202 is a response");

        assert_eq!(response.status, 202);
        assert_eq!(response.retry_after, Some(Duration::from_secs(5)));
        assert_eq!(response.rate_limit_remaining, Some(4999));
    }
}

/// A bounded quotation of something unparseable, so a diagnostic can be
/// specific without pasting an unbounded response into a log.
fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
