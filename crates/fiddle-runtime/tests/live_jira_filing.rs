mod support;

use fiddle_core::{
    AdvisoryId, DeploymentRule, EffectName, ProposedEffect, Severity, CVE_MITIGATE,
    JIRA_ISSUE_FILED,
};
use fiddle_runtime::cve::verdict::{
    ticket_proposals, Judgement, TicketFiling, TicketProposal, Verdict,
};
use fiddle_runtime::effect::{
    EffectContext, EffectTrace, ExecutionStep, Executor, IntegrationOperation, ReadRetry,
};
use fiddle_runtime::jira::file_verdict::FiledIssue;
use fiddle_runtime::jira::JiraHttp;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use support::{unreachable_context, Deployment};
use tokio_util::sync::CancellationToken;

const PROJECT: &str = "fiddle/live-jira-filing";

const ISSUE_TYPE: &str = "Task";

const CLOSING: &str = "Won't Do";

const CLOSING_FALLBACK: &str = "Done";

const PACKAGE: &str = "fiddle-live-jira-filing-lane";

const ADVISORY: &str = "CVE-0000-FIDDLE-LIVE-LANE";

const RATIONALE: &str = "This ticket was filed by fiddle's live filing lane to prove that \
                         FileVerdict can file against this site and recognise what it filed. \
                         It is not real work and the lane closes it in the same run.";

const LEGACY_LABEL: &str = "upstream-blocked";

const LAG_BOUND: Duration = Duration::from_secs(120);

const PROBE: &str = "fiddle-live-filing-probe";

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

struct Lane {
    site: String,
    project_key: String,
    ledger: String,
    invocation_ref: String,
    token: String,
    ctx: EffectContext,
    cancel: CancellationToken,
    answers: AtomicUsize,
}

struct Measured {
    filed: String,
    creates: usize,
    run_two_key: String,
    lag: Option<Lag>,
    held_after_two_runs: Vec<String>,
    found_by_search_inspect: Option<String>,
}

#[derive(Clone, Copy)]
enum Lag {
    AtMost { agreed: u128 },
    Between { disagreed: u128, agreed: u128 },
}

impl Lag {
    fn said(self) -> String {
        match self {
            Lag::AtMost { agreed } => format!("at most {agreed} ms, with no lower bound observed"),
            Lag::Between { disagreed, agreed } => {
                format!("more than {disagreed} ms and at most {agreed} ms")
            }
        }
    }
}

struct Swept {
    asked: Vec<String>,
    closed: Vec<String>,
    left_open: Vec<String>,
    claim_released: String,
}

fn named(variable: &str, why: &str) -> String {
    match std::env::var(variable) {
        Ok(held) if !held.trim().is_empty() => held,
        _ => panic!(
            "this lane needs {variable}. {why} It fails rather than skips, because a \
             silently-skipped lane cannot be told from a passing one."
        ),
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

impl Lane {
    fn opened() -> Self {
        let site = named(
            "JIRA_SITE",
            "It is the site origin, as in https://example.atlassian.net.",
        );
        let user = named(
            "JIRA_USER_EMAIL",
            "It is the account the writes are made as.",
        );
        let token = named(
            "JIRA_API_TOKEN",
            "It is the credential, and it is read from the environment and never written to a \
             file this lane makes.",
        );
        let project_key = named(
            "JIRA_WRITE_PROJECT",
            "It is the project this lane files one ticket in and then closes.",
        );
        let ledger = named(
            "JIRA_LEDGER_ISSUE",
            "It is an existing issue in JIRA_WRITE_PROJECT that carries the claim ledger. This \
             lane reads and writes properties on it and never closes it. Two rules bound it and \
             only the first is enforced here: it must name JIRA_WRITE_PROJECT, and it must \
             outlive every run, which no lane can check by reading one issue. A ticket an earlier \
             run filed and closed satisfies the second, because a closed issue still answers a \
             property read; a ticket this run will file does not, because closing it would take \
             the ledger with it. The property probe is the enforced half.",
        );

        assert!(
            site.starts_with("https://"),
            "JIRA_SITE must be an https origin and this is not one: {site}"
        );
        assert!(
            project_key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "JIRA_WRITE_PROJECT must be a bare project key and this is not one: {project_key}"
        );
        assert_eq!(
            ledger.split('-').next().unwrap_or_default(),
            project_key,
            "the ledger {ledger} is read with the same credential in the same project, and one \
             that names another project measures a workflow this lane never writes to"
        );
        if let Ok(observed) = std::env::var("JIRA_ISSUE") {
            assert_ne!(
                observed.split('-').next().unwrap_or_default(),
                project_key,
                "JIRA_WRITE_PROJECT is {project_key}, the project JIRA_ISSUE ({observed}) is read \
                 from. A project this lane writes to is not the project a read lane observes. \
                 Unset JIRA_ISSUE for this invocation rather than weakening the guard."
            );
        }

        let client = JiraHttp::new(&site, &user, &token, Duration::from_secs(60))
            .expect("a client for the named site is built");
        let ctx = unreachable_context().with_jira(client);
        let cancel = ctx.cancel.clone();

        Lane {
            site,
            project_key,
            ledger,
            invocation_ref: format!("live:{}-{}", unix_seconds(), std::process::id()),
            token,
            ctx,
            cancel,
            answers: AtomicUsize::new(0),
        }
    }

    fn client(&self) -> &JiraHttp {
        self.ctx.jira.as_ref().expect("this lane holds a client")
    }

    async fn read(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let answered = self
            .client()
            .api(method, path, body.as_ref(), &self.cancel)
            .await
            .unwrap_or_else(|error| panic!("the site would not answer {method} {path}: {error}"));
        self.answers.fetch_add(1, Ordering::SeqCst);
        assert!(
            !answered.body.to_string().contains(&self.token),
            "the site echoed the credential in its answer to {method} {path}, and this lane will \
             not print it"
        );
        (answered.status, answered.body)
    }

    fn answers_read(&self) -> usize {
        self.answers.load(Ordering::SeqCst)
    }

    fn issue_route(&self, key: &str) -> String {
        format!("/rest/api/3/issue/{key}")
    }

    fn claim_route(&self, marker: &str) -> String {
        format!("{}/properties/{marker}", self.issue_route(&self.ledger))
    }

    async fn keys_carrying(&self, marker: &str) -> Vec<String> {
        let jql = format!("project = {} AND labels = {marker}", self.project_key);
        let mut found = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..1000 {
            let mut path = format!(
                "/rest/api/3/search/jql?jql={}&fields=key",
                percent_encoded(&jql)
            );
            if let Some(next) = &token {
                path.push_str(&format!("&nextPageToken={}", percent_encoded(next)));
            }
            let (status, body) = self.read("GET", &path, None).await;
            assert_eq!(
                status, 200,
                "a search for `{jql}` answered HTTP {status}, so a count taken from it would be a \
                 count of nothing rather than a count of matches: {body}"
            );
            let page = body["issues"]
                .as_array()
                .unwrap_or_else(|| panic!("a search for `{jql}` answered no issues array: {body}"))
                .clone();
            for issue in page {
                let key = issue["key"].as_str().unwrap_or_else(|| {
                    panic!(
                        "a search answered an issue with no key while fields=key was asked, so \
                         the shape FileVerdict reads has changed: {issue}"
                    )
                });
                found.push(key.to_string());
            }
            match body["nextPageToken"].as_str() {
                None => {
                    found.sort();
                    found.dedup();
                    return found;
                }
                Some(next) => token = Some(next.to_string()),
            }
        }
        panic!(
            "the search for `{jql}` offered a further page after 1000 of them, and a count taken \
             from part of a result is a floor and never a total"
        );
    }

    fn verdicts(&self) -> Vec<Verdict> {
        vec![Verdict {
            cve: AdvisoryId::parse(ADVISORY).expect("the advisory id is not blank"),
            package: PACKAGE.to_string(),
            rationale: RATIONALE.to_string(),
            severity: Severity::Informational,
            verdict: Judgement::NeedsWork,
            legacy_label: Some(LEGACY_LABEL),
            disposed: None,
        }]
    }

    fn filing(&self) -> TicketFiling {
        TicketFiling {
            project_key: self.project_key.clone(),
            issue_type: ISSUE_TYPE.to_string(),
            ledger_issue: self.ledger.clone(),
        }
    }

    fn proposal(&self) -> TicketProposal {
        let filing = self.filing();
        let over = filing.over(PROJECT, &self.invocation_ref);
        let mut proposals = ticket_proposals(&self.verdicts(), &over);
        assert_eq!(
            proposals.len(),
            1,
            "one verdict carrying a legacy label proposes one ticket, and a lane that filed a \
             different number would be measuring a different thing"
        );
        proposals.pop().expect("one proposal is held")
    }

    async fn filed_through_the_executor(&self) -> Result<FiledIssue, String> {
        let proposal = self.proposal();
        let operation = proposal.operation();
        let deployment = Deployment(DeploymentRule::Allow);
        let trace = Silent;
        let executor = Executor::new(
            CVE_MITIGATE,
            PROJECT.to_string(),
            self.invocation_ref.clone(),
            &deployment,
            &self.ctx,
            &trace,
            ReadRetry::none(),
        );
        let proposed = ProposedEffect {
            capability: CVE_MITIGATE,
            kind: EffectName::shipped(JIRA_ISSUE_FILED),
            target: IntegrationOperation::target(&operation),
            payload: IntegrationOperation::payload(&operation),
        };
        executor
            .execute(proposed, operation)
            .await
            .map(|receipt| receipt.value)
            .map_err(|error| error.to_string())
    }

    async fn inspected(&self) -> Result<Option<FiledIssue>, String> {
        let operation = self.proposal().operation();
        IntegrationOperation::inspect(&operation, &self.ctx)
            .await
            .map_err(|error| error.to_string())
    }

    async fn closing_transition_on(&self, key: &str) -> Result<(String, String), String> {
        let (status, body) = self
            .read(
                "GET",
                &format!("{}/transitions", self.issue_route(key)),
                None,
            )
            .await;
        if status != 200 {
            return Err(format!(
                "the transitions of {key} answered HTTP {status}, so this lane cannot say whether \
                 it is able to close what it writes"
            ));
        }
        for wanted in [CLOSING, CLOSING_FALLBACK] {
            let ids: Vec<&str> = body["transitions"]
                .as_array()
                .map(|offered| {
                    offered
                        .iter()
                        .filter(|it| it["to"]["name"].as_str() == Some(wanted))
                        .filter_map(|it| it["id"].as_str())
                        .collect()
                })
                .unwrap_or_default();
            match ids.len() {
                0 => continue,
                1 => return Ok((ids[0].to_string(), wanted.to_string())),
                count => {
                    return Err(format!(
                        "{count} transitions on {key} reach {wanted}, and a close is sent as one \
                         id and never matched by category: fiddle-pu2c MEASURED that Won't Do and \
                         Done share the category done, so a category match cannot tell them apart"
                    ))
                }
            }
        }
        Err(format!(
            "no transition on {key} reaches {CLOSING} or {CLOSING_FALLBACK}, and a lane that \
             cannot close what it writes leaves residue on every run: {}",
            body["transitions"]
        ))
    }
}

fn percent_encoded(raw: &str) -> String {
    let mut written = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                written.push(*byte as char)
            }
            byte => written.push_str(&format!("%{byte:02X}")),
        }
    }
    written
}

async fn preconditions(lane: &Lane, marker: &str) -> Result<(), String> {
    let planted = json!({"decoy": format!("a decoy line carrying {} in the clear", lane.token)});
    if !planted.to_string().contains(&lane.token) {
        return Err(
            "the credential search did not find a planted credential, so its finding nothing in \
             the site's answers would prove nothing"
                .to_string(),
        );
    }
    println!(
        "live-jira-filing: the credential search finds a planted credential, so every absence \
         below is a measurement"
    );

    let (status, body) = lane
        .read("GET", &lane.issue_route(&lane.ledger), None)
        .await;
    if status != 200 {
        return Err(format!(
            "the ledger issue {} answered HTTP {status}. It must exist before this lane runs, \
             because a claim cannot be written on an issue the site does not hold.",
            lane.ledger
        ));
    }
    let held_type = body["fields"]["issuetype"]["name"]
        .as_str()
        .unwrap_or("absent");
    if held_type != ISSUE_TYPE {
        return Err(format!(
            "the ledger issue {} is a {held_type} and this lane files a {ISSUE_TYPE}. A Jira \
             workflow is per issue type, so a closing transition resolved on the ledger would say \
             nothing about the ticket this lane creates.",
            lane.ledger
        ));
    }
    let ledger_status = body["fields"]["status"]["name"]
        .as_str()
        .unwrap_or("absent");
    println!(
        "live-jira-filing: the ledger issue {} exists, is a {held_type} and reads as `{ledger_status}`",
        lane.ledger
    );

    let (id, name) = lane.closing_transition_on(&lane.ledger).await?;
    println!(
        "live-jira-filing: the closing transition resolves to exactly one id on this workflow: \
         {id} -> {name}"
    );

    let probe = format!("{}/properties/{PROBE}", lane.issue_route(&lane.ledger));
    let (written, _) = lane
        .read(
            "PUT",
            &probe,
            Some(json!({"probe": "the token can write a property"})),
        )
        .await;
    if !(200..300).contains(&written) {
        return Err(format!(
            "a property write on {} answered HTTP {written}. The claim ledger is the whole \
             exactly-once mechanism, and a run that discovers it cannot write one after it has \
             filed a ticket is the run that leaves residue.",
            lane.ledger
        ));
    }
    let (read_back, _) = lane.read("GET", &probe, None).await;
    if read_back != 200 {
        return Err(format!(
            "a property written on {} read back HTTP {read_back} with no wait, so this site does \
             not offer the immediate consistency the ledger rests on",
            lane.ledger
        ));
    }
    let (removed, _) = lane.read("DELETE", &probe, None).await;
    println!(
        "live-jira-filing: MEASURED a property on {} written, read back immediately and removed \
         (delete answered HTTP {removed})",
        lane.ledger
    );

    let (claimed, _) = lane.read("GET", &lane.claim_route(marker), None).await;
    if claimed != 404 {
        return Err(format!(
            "the claim {marker} already exists on {} (HTTP {claimed}), so this run's marker is \
             not unique to this run",
            lane.ledger
        ));
    }
    let already = lane.keys_carrying(marker).await;
    if !already.is_empty() {
        return Err(format!(
            "the marker {marker} already matches {already:?}, so this run's marker is not unique \
             to this run"
        ));
    }
    println!(
        "live-jira-filing: the marker {marker} matches nothing and the ledger holds no claim for \
         it, so every count below is this run's"
    );
    Ok(())
}

async fn measured(lane: &Lane, created: &mut Vec<String>) -> Result<Measured, String> {
    let marker = lane.proposal().marker().to_string();
    preconditions(lane, &marker).await?;

    let filed = lane.filed_through_the_executor().await?;
    if filed.marker != marker {
        return Err(format!(
            "the receipt names the marker {} and the proposal named {marker}",
            filed.marker
        ));
    }
    created.push(filed.key.clone());
    let filed_at = Instant::now();
    println!(
        "live-jira-filing: run one, through FileVerdict and the executor: the ledger held no \
         claim, so this run created {} and the claim now names it",
        filed.key
    );

    let (status, claim) = lane.read("GET", &lane.claim_route(&marker), None).await;
    if status != 200 || claim["value"]["filed"].as_str() != Some(filed.key.as_str()) {
        return Err(format!(
            "the claim on {} answered HTTP {status} and {} rather than naming {}",
            lane.ledger, claim, filed.key
        ));
    }

    let run_two = lane.filed_through_the_executor().await?;
    let creates = match run_two.key == filed.key {
        true => 1,
        false => 2,
    };
    println!(
        "live-jira-filing: run two, sent immediately with the same invocation ref, is the \
         interruption case: the executor's inspect answered {} and no create was sent",
        run_two.key
    );
    println!("live-jira-filing: MEASURED runs that created an issue: {creates} of 2");

    let mut held_after_two_runs = lane.keys_carrying(&marker).await;
    let mut agreed_at: Option<u128> = None;
    let mut disagreed_until: Option<u128> = None;
    loop {
        let at = filed_at.elapsed().as_millis();
        if held_after_two_runs.len() == creates && held_after_two_runs.contains(&filed.key) {
            agreed_at = Some(at);
            break;
        }
        if disagreed_until.is_none() {
            println!(
                "live-jira-filing: MEASURED issues the index shows for this marker {at} ms after \
                 the create: {} of {creates} created",
                held_after_two_runs.len()
            );
        }
        disagreed_until = Some(at);
        if filed_at.elapsed() >= LAG_BOUND {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        held_after_two_runs = lane.keys_carrying(&marker).await;
    }

    let lag = match (agreed_at, disagreed_until) {
        (Some(agreed), None) => {
            println!(
                "live-jira-filing: MEASURED the indexing lag is at most {agreed} ms and this lane \
                 cannot say what it was. The first search it sent, {agreed} ms after the create \
                 was accepted, already showed exactly the {creates} issue this run filed and it \
                 was {}. No search of this run disagreed, so no lower bound was observed and a \
                 lag of zero is not something this run measured.",
                filed.key
            );
            Some(Lag::AtMost { agreed })
        }
        (Some(agreed), Some(disagreed)) => {
            println!(
                "live-jira-filing: MEASURED the indexing lag is more than {disagreed} ms and at \
                 most {agreed} ms. The search at {disagreed} ms did not show exactly the \
                 {creates} issue this run filed and the search at {agreed} ms did, and it was {}. \
                 Both bounds come from searches whose counts this run compared with the number of \
                 creates it made.",
                filed.key
            );
            Some(Lag::Between { disagreed, agreed })
        }
        (None, _) => {
            println!(
                "live-jira-filing: the indexing lag is UNMEASURED: after {} ms the search still \
                 did not show exactly the {creates} issue this run filed, so the lag is longer \
                 than any wait this lane holds",
                LAG_BOUND.as_millis()
            );
            None
        }
    };

    let found_by_search_inspect = match lag {
        None => {
            println!(
                "live-jira-filing: NOT MEASURED: whether FileVerdict's search path reads the key \
                 off this site. The index never admitted the issue inside this lane's wait, so a \
                 search-backed inspect would have been read against an index this lane knows is \
                 stale."
            );
            None
        }
        Some(_) => {
            let (released, _) = lane.read("DELETE", &lane.claim_route(&marker), None).await;
            if released != 204 {
                return Err(format!(
                    "the claim for {marker} answered HTTP {released} to a delete, so the \
                     search-backed inspect below would have read the claim instead of the index"
                ));
            }
            println!(
                "live-jira-filing: the claim for {marker} was removed from {}, so the inspect \
                 below has nothing to read but the index",
                lane.ledger
            );
            let (gone, _) = lane.read("GET", &lane.claim_route(&marker), None).await;
            if gone != 404 {
                return Err(format!(
                    "the claim for {marker} still answers HTTP {gone} after a delete, so an \
                     inspect that found the ticket would prove nothing about the search"
                ));
            }
            let found = lane.inspected().await?.ok_or_else(|| {
                format!(
                    "with no claim on the ledger, FileVerdict::inspect searched this site for \
                     {marker} and found nothing, while the index shows {held_after_two_runs:?}"
                )
            })?;
            println!(
                "live-jira-filing: MEASURED with the claim removed, FileVerdict::inspect reached \
                 the search path and read {} off this site",
                found.key
            );
            Some(found.key)
        }
    };

    Ok(Measured {
        filed: filed.key,
        creates,
        run_two_key: run_two.key,
        lag,
        held_after_two_runs,
        found_by_search_inspect,
    })
}

async fn swept(lane: &Lane, marker: &str, created: &[String]) -> Swept {
    let mut asked: Vec<String> = created.to_vec();
    asked.extend(lane.keys_carrying(marker).await);
    asked.sort();
    asked.dedup();
    assert!(
        !asked.contains(&lane.ledger),
        "the close list names the ledger issue {}. The ledger outlives every run and is never \
         closed by one.",
        lane.ledger
    );

    let mut closed = Vec::new();
    let mut left_open = Vec::new();
    for key in &asked {
        let resolved = lane.closing_transition_on(key).await;
        let (id, name) = match resolved {
            Ok(held) => held,
            Err(why) => {
                println!("live-jira-filing: {key} was not closed: {why}");
                left_open.push(key.clone());
                continue;
            }
        };
        let (sent, body) = lane
            .read(
                "POST",
                &format!("{}/transitions", lane.issue_route(key)),
                Some(json!({"transition": {"id": id}})),
            )
            .await;
        if sent != 204 {
            println!(
                "live-jira-filing: the site would not close {key} through transition {id}: HTTP \
                 {sent}: {body}"
            );
            left_open.push(key.clone());
            continue;
        }
        let (_, held) = lane.read("GET", &lane.issue_route(key), None).await;
        let reached = held["fields"]["status"]["name"]
            .as_str()
            .unwrap_or("unreadable");
        if reached != name {
            println!(
                "live-jira-filing: {key} answered 204 to the close and reads back as {reached}, \
                 not {name}"
            );
            left_open.push(key.clone());
            continue;
        }
        println!(
            "live-jira-filing: closed {key} as {name} through transition {id}, verified by a \
             second read"
        );
        closed.push(key.clone());
    }

    let (released, _) = lane.read("DELETE", &lane.claim_route(marker), None).await;
    let claim_released = match released {
        204 => format!("the claim for {marker} was removed from {}", lane.ledger),
        404 => format!(
            "the ledger {} holds no claim for {marker}; the search-backed inspect above released \
             it and this delete confirmed it",
            lane.ledger
        ),
        other => format!(
            "the claim for {marker} answered HTTP {other} to a delete and remains on {}",
            lane.ledger
        ),
    };
    println!("live-jira-filing: {claim_released}");
    println!(
        "live-jira-filing: closed {} of {} issues this lane knows it wrote or matched",
        closed.len(),
        asked.len()
    );

    Swept {
        asked,
        closed,
        left_open,
        claim_released,
    }
}

#[tokio::test]
#[ignore = "writes to a real Jira site; run it through scripts/live-jira-file-verdict.sh with an \
            operator-supplied write token"]
async fn a_ticket_file_verdict_filed_is_found_by_a_later_inspect_against_the_real_site() {
    let lane = Lane::opened();
    let marker = lane.proposal().marker().to_string();
    println!(
        "live-jira-filing: site {}, project {}, ledger {}, marker {marker}",
        lane.site, lane.project_key, lane.ledger
    );

    let mut created: Vec<String> = Vec::new();
    let outcome = measured(&lane, &mut created).await;
    let sweep = swept(&lane, &marker, &created).await;

    if !sweep.left_open.is_empty() {
        println!(
            "live-jira-filing: LEFT OPEN in {} and not closed: {:?}. Close each by hand through \
             the {CLOSING} transition. Do not delete them: this project refuses a delete by \
             policy, and a run that inherits an open ticket carrying a live marker reads it as an \
             ambiguous match.",
            lane.project_key, sweep.left_open
        );
    }

    let held = match outcome {
        Ok(held) => held,
        Err(why) => panic!("live-jira-filing: FAIL: {why}"),
    };

    assert_eq!(
        held.creates, 1,
        "two runs of FileVerdict over one invocation sent {} creates, and exactly-once across an \
         interruption means exactly one. Run one filed {} and run two answered {}.",
        held.creates, held.filed, held.run_two_key
    );
    assert_eq!(
        held.run_two_key, held.filed,
        "run two's receipt must name the ticket run one filed, or the executor's inspect did not \
         recognise it"
    );
    assert_eq!(
        held.found_by_search_inspect.as_deref(),
        Some(held.filed.as_str()),
        "with the claim removed, FileVerdict::inspect must reach the search path and read the key \
         this site answers. A None here means the lag was never measured and the search path was \
         never driven; a different key means the search matched something this run did not file."
    );
    assert_eq!(
        held.held_after_two_runs,
        vec![held.filed.clone()],
        "the index must show exactly the one issue this run filed"
    );
    assert!(
        sweep.left_open.is_empty(),
        "the cleanup left {:?} open, and an open ticket carrying a live marker is the ambiguous \
         match the next run inherits",
        sweep.left_open
    );
    assert_eq!(
        sweep.closed.len(),
        sweep.asked.len(),
        "every issue this lane wrote or matched is closed, and {} of {} were",
        sweep.closed.len(),
        sweep.asked.len()
    );

    println!(
        "live-jira-filing: PASS: FileVerdict filed {} on {}, a second execution over the same \
         invocation recognised it and created nothing, a claim-free inspect found it through the \
         search, the lag was {}, and {}.",
        held.filed,
        lane.site,
        match held.lag {
            Some(lag) => lag.said(),
            None => "unmeasured".to_string(),
        },
        sweep.claim_released,
    );
    println!(
        "live-jira-filing: MEASURED the credential reached none of the {} answers this lane read \
         itself. The answers FileVerdict read are not among them: JiraHttp scrubs its own bodies, \
         and a_write_carries_the_credential_nowhere_a_reader_sees measures that against the stub.",
        lane.answers_read()
    );
}
