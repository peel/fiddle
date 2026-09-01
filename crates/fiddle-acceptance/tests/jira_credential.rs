mod support;

use std::path::{Path, PathBuf};
use support::{Reply, Scenario, StubGateway, StubJira};

const TOKEN_CREDENTIAL: &str = "JIRA_API_TOKEN";

const USER_CREDENTIAL: &str = "JIRA_USER_EMAIL";

const SENTINEL: &str = "jira-sentinel-do-not-log";

const USER: &str = "bot@example.com";

const KEY: &str = support::JIRA_ISSUE_KEY;

const REFERENCE: &str = "jira:IDENT-1";

const SITE: &str = "https://icecube.atlassian.net";

const UNREACHABLE: &str = "http://127.0.0.1:9";

const ATTEMPT: &str = "<attempt>";

const CENSUS: [&str; 42] = [
    "a sweep files a ticket / run cve --json / nothing on stderr",
    "a sweep files a ticket / run cve --json / the file `reports/cve/<attempt>/report.json`",
    "a sweep files a ticket / run cve --json / the file `reports/filings.json`",
    "a sweep files a ticket / run cve --json / the file `reports/findings.json`",
    "a sweep files a ticket / run cve --json / the file `reports/rescan/child.json`",
    "a sweep files a ticket / run cve --json / the file `reports/rescan/scan.json`",
    "a sweep files a ticket / run cve --json / the file `reports/scan/child.json`",
    "a sweep files a ticket / run cve --json / the file `reports/scan/scan.json`",
    "a sweep files a ticket / run cve --json / the file `reports/verdicts.json`",
    "a sweep files a ticket / run cve --json / what it printed on stdout",
    "half a credential / inspect --json / a diagnostic on stderr",
    "half a credential / inspect --json / nothing on stdout",
    "half a credential / inspect --json / the file `fiddle.toml`",
    "the document is read back / config check --json / nothing on stderr",
    "the document is read back / config check --json / the file `fiddle.toml`",
    "the document is read back / config check --json / what it printed on stdout",
    "the document is read back / config check / nothing on stderr",
    "the document is read back / config check / the file `fiddle.toml`",
    "the document is read back / config check / what it printed on stdout",
    "the filing table is read back / config check --json / nothing on stderr",
    "the filing table is read back / config check --json / the file `fiddle.toml`",
    "the filing table is read back / config check --json / what it printed on stdout",
    "the filing table is read back / config check / nothing on stderr",
    "the filing table is read back / config check / the file `fiddle.toml`",
    "the filing table is read back / config check / what it printed on stdout",
    "the site answers / inspect --json / nothing on stderr",
    "the site answers / inspect --json / what it printed on stdout",
    "the site answers / inspect / nothing on stderr",
    "the site answers / inspect / what it printed on stdout",
    "the site answers / run --json / nothing on stderr",
    "the site answers / run --json / the file `fiddle.toml`",
    "the site answers / run --json / the file `reports/jira-IDENT-1/<attempt>/report.json`",
    "the site answers / run --json / the file `stub-state/changes/IDENT-1.json`",
    "the site answers / run --json / what it printed on stdout",
    "the site cannot be reached / inspect --json / nothing on stderr",
    "the site cannot be reached / inspect --json / what it printed on stdout",
    "the site cannot be reached / run --json / nothing on stderr",
    "the site cannot be reached / run --json / the file `fiddle.toml`",
    "the site cannot be reached / run --json / the file `reports/jira-IDENT-1/<attempt>/report.json`",
    "the site cannot be reached / run --json / what it printed on stdout",
    "the site refuses the credential / inspect --json / nothing on stderr",
    "the site refuses the credential / inspect --json / what it printed on stdout",
];

const QUALIFIED: [&str; 3] = [
    "the site answers / inspect --json / what it printed on stdout",
    "the site answers / run --json / the file `reports/jira-IDENT-1/<attempt>/report.json`",
    "the site answers / run --json / what it printed on stdout",
];

const PROJECTS: usize = 6;

const SWEEP_REF: &str = "cve";

const SWEEP_REPO: &str = "acme/r";

const SWEEP_BASE: &str = "main";

const SWEEP_IMAGE: &str = "ghcr.io/acme/icecube:latest";

const SWEEP_FIXTURE: &str = "cve-vulnerable";

const SWEEP_SCAN: &str = "no-published-fix";

const SWEEP_RESCAN: &str = "library-clean";

const SWEEP_CVE: &str = "CVE-2026-0001";

const SWEEP_DECLINED: &str = "no published fix I can apply without a registry";

const FILED_ISSUE: &str = "SEC-42";

const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";

const MODEL_KEY: &str = "LITELLM_API_KEY";

const WIZ_ID: &str = "WIZ_CLIENT_ID";

const WIZ_SECRET: &str = "WIZ_CLIENT_SECRET";

const FILING_PROJECT: &str = "SEC";

const FILING_ISSUE_TYPE: &str = "Task";

const FILING_LEDGER: &str = "SEC-1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Presence {
    Written,
    Spoken,
    Silent,
}

struct Surface {
    what: String,
    text: String,
    presence: Presence,
    path: Option<PathBuf>,
}

impl Surface {
    fn proved(&self) -> &str {
        match self.presence {
            Presence::Written => {
                let path = self
                    .path
                    .as_ref()
                    .unwrap_or_else(|| panic!("{} is a file and must name it", self.what));
                assert!(
                    path.is_file(),
                    "{} names {} and no such file exists, so searching it would be \
                     searching an empty string and the search would pass for a reason \
                     that has nothing to do with the credential",
                    self.what,
                    path.display()
                );
                let bytes = std::fs::metadata(path)
                    .unwrap_or_else(|e| panic!("could not stat {} ({e})", path.display()))
                    .len();
                assert!(
                    bytes > 0,
                    "{} names {}, which exists and holds nothing; an empty file cannot \
                     hold a credential and cannot witness that it holds none",
                    self.what,
                    path.display()
                );
                assert_eq!(
                    self.text.len() as u64,
                    bytes,
                    "{} was read short of {}",
                    self.what,
                    path.display()
                );
            }
            Presence::Spoken => assert!(
                !self.text.trim().is_empty(),
                "{} carried no words, so it witnesses nothing about a credential",
                self.what
            ),
            Presence::Silent => assert!(
                self.text.is_empty(),
                "{} was expected to carry nothing and carried {:?}; a stream that has \
                 started speaking has to be searched deliberately rather than pass as \
                 an empty string",
                self.what,
                self.text
            ),
        }
        &self.text
    }
}

struct Needle {
    what: &'static str,
    text: String,
}

struct Searched {
    surfaces: Vec<Surface>,
    needles: Vec<Needle>,
    projects: Vec<Scenario>,
    sites: Vec<StubJira>,
    filed: Filed,
}

impl Searched {
    fn census(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .surfaces
            .iter()
            .map(|surface| surface.what.clone())
            .collect();
        names.sort();
        names
    }

    fn leaking(&self) -> Vec<String> {
        let mut found = Vec::new();
        for surface in &self.surfaces {
            let text = surface.proved();
            for needle in &self.needles {
                if text.contains(&needle.text) {
                    found.push(format!("{} carries {}", surface.what, needle.what));
                }
            }
        }
        found
    }
}

#[test]
fn no_surface_a_reader_sees_carries_the_jira_credential() {
    let searched = surfaces_of_every_jira_invocation();

    assert_eq!(
        searched.census(),
        CENSUS,
        "the set of surfaces searched is pinned here, so a surface this build starts \
         writing cannot join the tree unsearched and a surface it stops writing cannot \
         leave the search passing on an absent file"
    );
    let token = searched
        .needles
        .iter()
        .find(|needle| needle.what.contains("token"))
        .expect("the token an operator exported has to be one of the needles");
    let header = searched
        .needles
        .iter()
        .find(|needle| needle.what.contains("header"))
        .expect("the header the credential is encoded into has to be one of the needles");
    assert!(
        !token.text.is_empty() && !header.text.is_empty(),
        "an empty needle is carried by every surface, so it reports a leak on a \
         surface that holds nothing: the token is {:?} and the header is {:?}",
        token.text,
        header.text
    );
    assert!(
        !header.text.contains(&token.text) && !token.text.contains(&header.text),
        "the credential reaches a surface either as the token an operator exported or \
         as the header it is encoded into, and neither needle may contain the other, \
         or a search for one stands in for a search for the other: the token is {:?} \
         and the header is {:?}",
        token.text,
        header.text
    );

    assert_eq!(
        searched.leaking(),
        Vec::<String>::new(),
        "the jira credential reached a surface a reader sees"
    );
}

#[test]
fn every_surface_searched_is_output_of_a_jira_read() {
    let searched = surfaces_of_every_jira_invocation();

    let answering = &searched.sites[0];
    let refusing = &searched.sites[1];
    assert_eq!(
        answering.the_only_authorization(),
        refusing.the_only_authorization(),
        "a site that answers and a site that refuses are different code paths, and \
         the needle this lane searches for is the header the site that answers \
         received, so the site that refuses has to have received that same header or \
         the search is blind to what the refusing path carried; the left header \
         reached the site that answers and the right one the site that refuses"
    );

    for project in &searched.projects {
        assert!(
            project.dir().is_dir(),
            "{} was searched and no longer exists",
            project.dir().display()
        );
    }

    let speaking: Vec<&Surface> = searched
        .surfaces
        .iter()
        .filter(|surface| surface.presence != Presence::Silent)
        .collect();
    assert!(
        speaking.len() >= 12,
        "too few surfaces carry words for this lane to be evidence of anything: {:?}",
        speaking.iter().map(|s| &s.what).collect::<Vec<_>>()
    );

    let naming: Vec<&String> = speaking
        .iter()
        .filter(|surface| surface.text.contains(SITE) && surface.text.contains(KEY))
        .map(|surface| &surface.what)
        .collect();
    assert!(
        naming.len() >= 6,
        "a surface that never names the site or the issue is not output of a jira \
         read, and searching it proves nothing about a jira credential; only \
         {naming:?} name both"
    );

    let carrying = |held: &str| {
        let mut named: Vec<String> = searched
            .surfaces
            .iter()
            .filter(|surface| surface.text.contains(held))
            .map(|surface| surface.what.clone())
            .collect();
        named.sort();
        named
    };
    for (field, held) in [
        ("labels", support::JIRA_ISSUE_LABEL),
        ("description", support::JIRA_ISSUE_DESCRIPTION),
        ("comment", support::JIRA_ISSUE_COMMENT),
    ] {
        assert_eq!(
            carrying(held),
            QUALIFIED,
            "the read asks the site for `{field}` so a gate can weigh it, and every \
             surface that value reaches is pinned here and searched for the credential; \
             a surface it starts reaching cannot join the tree unsearched"
        );
    }

    let published: Vec<&String> = searched
        .surfaces
        .iter()
        .filter(|surface| surface.presence == Presence::Written)
        .map(|surface| &surface.what)
        .collect();
    assert!(
        published.len() >= 6,
        "the credential must be looked for in what the process wrote and not only in \
         what it said, and only {published:?} are files"
    );

    let filing: Vec<&String> = searched
        .surfaces
        .iter()
        .filter(|surface| surface.what.starts_with("a sweep files a ticket / "))
        .map(|surface| &surface.what)
        .collect();
    assert!(
        filing.iter().any(|what| what.contains("filings.json"))
            && filing.iter().any(|what| what.ends_with("on stdout")),
        "the census has to name both what the filing run said and the filing report it \
         wrote, which is the file such a run publishes and no reading run does; these \
         are the surfaces it named: {filing:?}"
    );
    assert_eq!(
        searched.filed.filings()["tickets"][0]["issue"],
        serde_json::json!(FILED_ISSUE),
        "and that report has to name the issue the site created, or the run those \
         surfaces came from took the filing path and filed nothing, which is what a \
         run that only reads a tracker also does"
    );
}

#[test]
fn the_same_search_finds_the_credential_when_a_surface_does_carry_it() {
    let mut searched = surfaces_of_every_jira_invocation();
    assert_eq!(
        searched.leaking(),
        Vec::<String>::new(),
        "this build must be clean before anything is planted in it"
    );

    let published = searched
        .surfaces
        .iter()
        .filter(|surface| surface.presence == Presence::Written)
        .filter_map(|surface| surface.path.clone())
        .find(|path| path.file_name().is_some_and(|name| name == "report.json"))
        .expect("a run published a bundle");
    let beside = published
        .parent()
        .expect("a published bundle sits in a directory")
        .join("a-second-file.json");
    std::fs::write(&beside, format!("{{\"token\":\"{SENTINEL}\"}}")).unwrap();

    let root = published
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "reports"))
        .expect("a published bundle sits under the report directory");
    searched
        .surfaces
        .extend(files_under("a plant", root, root.parent().unwrap()));

    let header = searched
        .needles
        .iter()
        .find(|needle| needle.what.contains("header"))
        .expect("the header is one of the needles")
        .text
        .clone();
    searched.surfaces.push(Surface {
        what: "a plant / a stream".to_string(),
        text: format!("the site could not be reached: authorization {header}"),
        presence: Presence::Spoken,
        path: None,
    });

    let leaking = searched.leaking();
    assert!(
        leaking
            .iter()
            .any(|reported| reported.contains("a-second-file.json")),
        "the walk has to reach a file that was not there when the census was written, \
         or a leak into a second published file would never be looked at: {leaking:?}"
    );
    assert!(
        leaking
            .iter()
            .any(|reported| reported.contains("a plant / a stream")),
        "the search has to see the encoded header and not only the raw token: \
         {leaking:?}"
    );
}

#[test]
fn the_authorization_the_site_receives_is_derived_from_the_credential_and_hides_it() {
    let held = authorization_for(SENTINEL);
    let again = authorization_for(SENTINEL);
    let other = authorization_for("jira-a-different-token-entirely");

    assert!(
        held.starts_with("Basic "),
        "the port authenticates with a basic header, and the blob below is the half \
         of it that carries the credential: {held}"
    );
    assert_eq!(
        held, again,
        "one credential has to encode to one header, or the needle read off the wire \
         is not the needle the next invocation would leak"
    );
    assert_ne!(
        held, other,
        "the header has to change when the token changes, or it is not derived from \
         the credential and searching for it proves nothing"
    );
    assert!(
        !held.contains(SENTINEL),
        "the header does not carry the token in the letters an operator exported, \
         which is why a search for the token alone would not see a leaked header: \
         {held}"
    );
}

fn authorization_for(token: &str) -> String {
    let stub = StubJira::holding_the_issue();
    let scenario = Scenario::new();
    scenario.append_config(&jira_table(&stub.base_url()));

    let said = ask(
        &scenario,
        &["inspect", REFERENCE, "--json"],
        &[(USER_CREDENTIAL, USER), (TOKEN_CREDENTIAL, token)],
    );
    assert_eq!(
        said.code,
        Some(0),
        "the read has to reach the site for a header to have been received: {}",
        said.stderr
    );
    stub.the_only_authorization()
}

fn surfaces_of_every_jira_invocation() -> Searched {
    let mut surfaces = Vec::new();
    let credentials = [(USER_CREDENTIAL, USER), (TOKEN_CREDENTIAL, SENTINEL)];

    let sites = vec![
        StubJira::holding_the_issue(),
        StubJira::refusing_the_credential(),
        StubJira::filing_as(FILED_ISSUE),
    ];
    let projects: Vec<Scenario> = (0..PROJECTS).map(|_| Scenario::new()).collect();
    let sited = [
        sites[0].base_url(),
        sites[1].base_url(),
        UNREACHABLE.to_string(),
        UNREACHABLE.to_string(),
        UNREACHABLE.to_string(),
        UNREACHABLE.to_string(),
    ];
    for (project, base_url) in projects.iter().zip(&sited) {
        project.append_config(&jira_table(base_url));
    }
    let answering = &sites[0];
    let refusing = &sites[1];
    let filed_into = &sites[2];
    let answered = &projects[0];
    let refused = &projects[1];
    let unreachable = &projects[2];
    let halved = &projects[3];
    let echoed = &projects[4];
    let filing = &projects[5];
    filing.append_config(&filing_table());

    for (command, args) in [
        ("inspect --json", vec!["inspect", REFERENCE, "--json"]),
        ("inspect", vec!["inspect", REFERENCE]),
        (
            "run --json",
            vec!["run", REFERENCE, "--capability", "stub_mark", "--json"],
        ),
    ] {
        let said = ask(answered, &args, &credentials);
        assert_eq!(
            said.code,
            Some(0),
            "`{command}` against a site that answers must succeed, or the surfaces \
             below are a refusal's and not a read's: {}",
            said.stderr
        );
        surfaces.extend(said.streams("the site answers", command));
    }
    assert!(
        answering.served() >= 3,
        "the site was asked {} times and three invocations reached it, so a \
         credential never travelled",
        answering.served()
    );
    surfaces.extend(files_of("the site answers", "run --json", answered.dir()));

    let said = ask(refused, &["inspect", REFERENCE, "--json"], &credentials);
    assert!(
        said.stdout
            .contains("the site refused the credential with 401"),
        "the site must have refused the credential for this surface to be the \
         diagnostic that names it: {}",
        said.stdout
    );
    assert_eq!(
        refusing.served(),
        1,
        "the refusal has to have been the site's and not a client's"
    );
    surfaces.extend(said.streams("the site refuses the credential", "inspect --json"));

    for (command, args) in [
        ("inspect --json", vec!["inspect", REFERENCE, "--json"]),
        (
            "run --json",
            vec!["run", REFERENCE, "--capability", "stub_mark", "--json"],
        ),
    ] {
        let said = ask(unreachable, &args, &credentials);
        assert!(
            said.stdout.contains("the site could not be reached"),
            "`{command}` must reach the unreachable arm, which is the one that \
             quotes what the client said and so the one a leak escapes through: {}",
            said.stdout
        );
        surfaces.extend(said.streams("the site cannot be reached", command));
    }
    surfaces.extend(files_of(
        "the site cannot be reached",
        "run --json",
        unreachable.dir(),
    ));

    let said = ask(
        halved,
        &["inspect", REFERENCE, "--json"],
        &[(TOKEN_CREDENTIAL, SENTINEL)],
    );
    assert_eq!(
        said.code,
        Some(2),
        "a document naming a variable nothing exports is invalid configuration: {}",
        said.stdout
    );
    assert!(
        said.stderr.contains(USER_CREDENTIAL),
        "the refusal must name the half that is missing, so the surface below is a \
         diagnostic written while the other half was exported: {}",
        said.stderr
    );
    surfaces.extend(said.streams("half a credential", "inspect --json"));
    surfaces.extend(files_of(
        "half a credential",
        "inspect --json",
        halved.dir(),
    ));

    for (command, args) in [
        ("config check", vec!["config", "check"]),
        ("config check --json", vec!["config", "check", "--json"]),
    ] {
        let said = ask(echoed, &args, &credentials);
        assert_eq!(
            said.code,
            Some(0),
            "`{command}` must read the document back: {}",
            said.stderr
        );
        assert!(
            said.stdout.contains(TOKEN_CREDENTIAL),
            "`{command}` names the variable the document points at, which is the \
             surface most likely to print its value beside it: {}",
            said.stdout
        );
        surfaces.extend(said.streams("the document is read back", command));
        surfaces.extend(files_of("the document is read back", command, echoed.dir()));
    }

    for (command, args) in [
        ("config check", vec!["config", "check"]),
        ("config check --json", vec!["config", "check", "--json"]),
    ] {
        let said = ask(filing, &args, &credentials);
        assert_eq!(
            said.code,
            Some(0),
            "`{command}` must read a document that files back: {}",
            said.stderr
        );
        assert!(
            said.stdout.contains(FILING_LEDGER) && said.stdout.contains(FILING_ISSUE_TYPE),
            "`{command}` must echo the ledger issue and the issue type the filing table \
             names, or the surface below is a tracker read's and says nothing about the \
             path that writes: {}",
            said.stdout
        );
        surfaces.extend(said.streams("the filing table is read back", command));
        surfaces.extend(files_of(
            "the filing table is read back",
            command,
            filing.dir(),
        ));
    }

    let filed = a_sweep_that_files(filed_into, &credentials);
    assert_eq!(
        filed.said.code,
        Some(0),
        "the sweep has to succeed, or the surfaces below are a failed run's and the \
         filing path is not what wrote them: {}",
        filed.said.stderr
    );
    assert_eq!(
        filed.filings()["tickets"][0]["state"],
        serde_json::json!("filed"),
        "the run has to have filed a ticket, or no jira credential travelled and \
         searching what it wrote proves nothing: {}",
        filed.filings()
    );
    let created: Vec<String> = filed_into
        .request_lines()
        .into_iter()
        .filter(|line| line.starts_with("POST /rest/api/3/issue "))
        .collect();
    assert_eq!(
        created.len(),
        1,
        "one advisory went unrepaired, so the site received exactly one create; more or \
         fewer and the run below did not take the filing path once: {created:?}"
    );
    assert_eq!(
        filed_into.the_only_authorization(),
        answering.the_only_authorization(),
        "the needle this lane searches for is the header the site that answers \
         received, so the site that was filed into has to have received that same \
         header or the search of a filing run's surfaces is blind"
    );
    surfaces.extend(
        filed
            .said
            .streams("a sweep files a ticket", "run cve --json"),
    );
    surfaces.extend(files_under(
        "a sweep files a ticket / run cve --json",
        &filed.scenario.report_dir(),
        filed.scenario.dir(),
    ));

    let needles = vec![
        Needle {
            what: "the token an operator exported",
            text: SENTINEL.to_string(),
        },
        Needle {
            what: "the header the credential is encoded into",
            text: answering
                .the_only_authorization()
                .trim_start_matches("Basic ")
                .to_string(),
        },
    ];

    Searched {
        surfaces,
        needles,
        projects,
        sites,
        filed,
    }
}

struct Said {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Said {
    fn streams(&self, scenario: &str, command: &str) -> Vec<Surface> {
        [("stdout", &self.stdout), ("stderr", &self.stderr)]
            .into_iter()
            .map(|(stream, text)| {
                let (presence, said) = match text.is_empty() {
                    true => (Presence::Silent, format!("nothing on {stream}")),
                    false => match stream {
                        "stdout" => (Presence::Spoken, "what it printed on stdout".to_string()),
                        _ => (Presence::Spoken, "a diagnostic on stderr".to_string()),
                    },
                };
                Surface {
                    what: format!("{scenario} / {command} / {said}"),
                    text: text.clone(),
                    presence,
                    path: None,
                }
            })
            .collect()
    }
}

fn files_of(scenario: &str, command: &str, root: &Path) -> Vec<Surface> {
    files_under(&format!("{scenario} / {command}"), root, root)
}

fn files_under(prefix: &str, root: &Path, relative_to: &Path) -> Vec<Surface> {
    let found = support::walkdir_files(root);
    assert!(
        !found.is_empty(),
        "{} holds no file at all, so `{prefix}` searched nothing it wrote",
        root.display()
    );
    found
        .into_iter()
        .map(|path| {
            let named = path
                .strip_prefix(relative_to)
                .unwrap_or(&path)
                .components()
                .map(
                    |part| match is_attempt_id(part.as_os_str().to_string_lossy().as_ref()) {
                        true => ATTEMPT.to_string(),
                        false => part.as_os_str().to_string_lossy().into_owned(),
                    },
                )
                .collect::<Vec<_>>()
                .join("/");
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()));
            Surface {
                what: format!("{prefix} / the file `{named}`"),
                text: String::from_utf8_lossy(&bytes).into_owned(),
                presence: Presence::Written,
                path: Some(path),
            }
        })
        .collect()
}

fn is_attempt_id(segment: &str) -> bool {
    segment.len() == 26
        && segment
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
}

fn jira_table(base_url: &str) -> String {
    format!(
        "[jira]\n\
         site = \"{SITE}\"\n\
         project = \"IDENT\"\n\
         user = {{ env = \"{USER_CREDENTIAL}\" }}\n\
         token = {{ env = \"{TOKEN_CREDENTIAL}\" }}\n\
         base_url = \"{base_url}\"\n\
         timeout = \"30s\"\n"
    )
}

fn filing_table() -> String {
    format!(
        "\n[jira.filing]\n\
         project = \"{FILING_PROJECT}\"\n\
         issue_type = \"{FILING_ISSUE_TYPE}\"\n\
         ledger_issue = \"{FILING_LEDGER}\"\n"
    )
}

struct Filed {
    scenario: Scenario,
    said: Said,
}

impl Filed {
    fn filings(&self) -> serde_json::Value {
        let path = self.scenario.report_dir().join("filings.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("no filing report at {} ({e})", path.display()));
        serde_json::from_slice(&bytes).unwrap()
    }
}

fn a_sweep_that_files(site: &StubJira, credentials: &[(&str, &str)]) -> Filed {
    let scenario = Scenario::new();
    let stub = scenario.dir().join("gh-stub");
    std::fs::create_dir_all(stub.join("script")).unwrap();
    std::fs::create_dir_all(stub.join("config")).unwrap();
    let remote = stub.join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    support::git(&remote, &["init", "-q", "--bare", "-b", SWEEP_BASE, "."]);

    let tree = seed_repository(scenario.dir(), &remote);
    let gateway = StubGateway::serving(a_script_declining_the_only_advisory());
    let login = support::caller_logged_in();

    scenario.append_config(&sweep_tables(
        &stub,
        &tree,
        &scenario.dir().join("workspaces"),
        &gateway.base_url(),
    ));
    scenario.append_config(&jira_table(&site.base_url()));
    scenario.append_config(&filing_table());

    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS
        .iter()
        .chain([USER_CREDENTIAL, FORGE_TOKEN, MODEL_KEY, WIZ_ID, WIZ_SECRET].iter())
    {
        command.env_remove(name);
    }
    command
        .args(["run", SWEEP_REF])
        .args(["--capability", "cve_mitigate"])
        .args(["--config", scenario.config_path().to_str().unwrap()])
        .arg("--json")
        .env(FORGE_TOKEN, "ghp_forge_token_for_the_sweep")
        .env(MODEL_KEY, "sk-model-key-for-the-sweep")
        .env(WIZ_ID, "wiz-client-id-for-the-sweep")
        .env(WIZ_SECRET, "wiz-client-secret-for-the-sweep")
        .env(support::WIZ_CONFIG_DIR, login.path());
    for (name, value) in credentials {
        command.env(name, value);
    }
    let out = command.output().unwrap();
    assert!(
        gateway.served() >= 1,
        "the agent was never consulted, so no advisory was dispositioned and the run \
         below reached no filing verdict"
    );

    Filed {
        scenario,
        said: Said {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
    }
}

fn seed_repository(root: &Path, remote: &Path) -> PathBuf {
    let tree = root.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    for (path, bytes) in support::tracked_files(&support::fixture(SWEEP_FIXTURE)) {
        let destination = tree.join(&path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(destination, bytes).unwrap();
    }
    support::git(&tree, &["init", "-q", "-b", SWEEP_BASE, "."]);
    support::git(&tree, &["add", "-A"]);
    support::git(
        &tree,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the fixture under remediation",
        ],
    );
    support::git(
        &tree,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    support::git(&tree, &["push", "-q", "origin", SWEEP_BASE]);
    tree
}

fn a_script_declining_the_only_advisory() -> Vec<Reply> {
    vec![support::accepted(support::completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": SWEEP_DECLINED,
                "claimed_complete": false,
                "findings": [{
                    "cve": SWEEP_CVE,
                    "attempted": false,
                    "note": SWEEP_DECLINED,
                }],
            }).to_string(),
        }),
        "stop",
    ))]
}

fn sweep_tables(stub: &Path, tree: &Path, workspaces: &Path, base_url: &str) -> String {
    format!(
        "[github]\n\
         repo = \"{SWEEP_REPO}\"\n\
         base = \"{SWEEP_BASE}\"\n\
         token = {{ env = \"{FORGE_TOKEN}\" }}\n\
         cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
         git = \"git\"\n\
         config_dir = {config_dir}\n\
         timeout = \"120s\"\n\
         \n\
         [agent]\n\
         model = \"a-model\"\n\
         base_url = \"{base_url}\"\n\
         api_key = {{ env = \"{MODEL_KEY}\" }}\n\
         max_turns = 6\n\
         max_tokens = 512\n\
         max_changed_files = 4\n\
         deadline = \"300s\"\n\
         tool_timeout = \"300s\"\n\
         \n\
         [scanner]\n\
         cli = {{ program = {wiz}, args = [\"{SWEEP_SCAN}\"] }}\n\
         timeout = \"300s\"\n\
         \n\
         [orchestration.cve]\n\
         image = \"{SWEEP_IMAGE}\"\n\
         max_findings = 2\n\
         \n\
         [workspace]\n\
         root = {workspaces}\n\
         fixture = {tree}\n\
         command_timeout = \"300s\"\n\
         \n\
         [[workspace.checks]]\n\
         program = {check}\n\
         args = []\n\
         success = \"exit-zero\"\n\
         \n\
         [[workspace.checks]]\n\
         program = {wiz}\n\
         args = [\"{SWEEP_RESCAN}\"]\n\
         success = \"artefact-written\"\n",
        gh = support::toml_string(support::gh_stub_binary()),
        wiz = support::toml_string(support::wiz_stub_binary()),
        check = support::toml_string(support::check_stub_binary()),
        stub = support::toml_string(stub),
        config_dir = support::toml_string(&stub.join("config")),
        workspaces = support::toml_string(workspaces),
        tree = support::toml_string(tree),
    )
}

fn ask(scenario: &Scenario, args: &[&str], credentials: &[(&str, &str)]) -> Said {
    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    command.env_remove(USER_CREDENTIAL);
    command.args(args);
    command.args(["--config", scenario.config_path().to_str().unwrap()]);
    for (name, value) in credentials {
        command.env(name, value);
    }
    let out = command.output().unwrap();
    Said {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}
