mod support;

use std::path::{Path, PathBuf};
use support::{Scenario, StubJira};

const TOKEN_CREDENTIAL: &str = "JIRA_API_TOKEN";

const USER_CREDENTIAL: &str = "JIRA_USER_EMAIL";

const SENTINEL: &str = "jira-sentinel-do-not-log";

const USER: &str = "bot@example.com";

const KEY: &str = support::JIRA_ISSUE_KEY;

const REFERENCE: &str = "jira:IDENT-1";

const SITE: &str = "https://icecube.atlassian.net";

const UNREACHABLE: &str = "http://127.0.0.1:9";

const ATTEMPT: &str = "<attempt>";

const CENSUS: [&str; 26] = [
    "half a credential / inspect --json / a diagnostic on stderr",
    "half a credential / inspect --json / nothing on stdout",
    "half a credential / inspect --json / the file `fiddle.toml`",
    "the document is read back / config check --json / nothing on stderr",
    "the document is read back / config check --json / the file `fiddle.toml`",
    "the document is read back / config check --json / what it printed on stdout",
    "the document is read back / config check / nothing on stderr",
    "the document is read back / config check / the file `fiddle.toml`",
    "the document is read back / config check / what it printed on stdout",
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

const PROJECTS: usize = 5;

const SITES: usize = 2;

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
    assert_eq!(
        searched.needles.len(),
        2,
        "the credential reaches a surface either as the token an operator exported or \
         as the header it is encoded into, and a search for one of those is blind to \
         the other"
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

    assert_eq!(
        searched.projects.len(),
        PROJECTS,
        "each disposable project has to outlive the search, or a file surface is read \
         from a directory that has already been deleted and reads as empty"
    );
    assert_eq!(
        searched.sites.len(),
        SITES,
        "a site that answers and a site that refuses are different code paths and \
         both carry the credential"
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
    ];
    let projects: Vec<Scenario> = (0..PROJECTS).map(|_| Scenario::new()).collect();
    let sited = [
        sites[0].base_url(),
        sites[1].base_url(),
        UNREACHABLE.to_string(),
        UNREACHABLE.to_string(),
        UNREACHABLE.to_string(),
    ];
    for (project, base_url) in projects.iter().zip(&sited) {
        project.append_config(&jira_table(base_url));
    }
    let answering = &sites[0];
    let refusing = &sites[1];
    let answered = &projects[0];
    let refused = &projects[1];
    let unreachable = &projects[2];
    let halved = &projects[3];
    let echoed = &projects[4];

    for (command, args) in [
        ("inspect --json", vec!["inspect", REFERENCE, "--json"]),
        ("inspect", vec!["inspect", REFERENCE]),
        ("run --json", vec!["run", REFERENCE, "--json"]),
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
        ("run --json", vec!["run", REFERENCE, "--json"]),
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
