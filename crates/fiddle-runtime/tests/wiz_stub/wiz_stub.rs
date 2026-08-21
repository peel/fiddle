#[allow(dead_code)]
#[path = "../support/document.rs"]
mod document;

use document::{
    libraries, libraries_carrying, libraries_graded, os_packages, python_libraries, report_with,
    unfixed_libraries, DEFAULT_LIBRARY_CVES, DEFAULT_OS_CVES, DIGEST_ON_STDOUT,
    FIXTURE_CLIENT_VERSION, SECOND_LIBRARY_CVE, SECOND_OS_CVE,
};
use std::path::{Path, PathBuf};

const REAL_SHAPE: &str = include_str!("../../../../tests/fixtures/wiz-real/wiz.json");

const VERSION_ON_STDOUT: &str = "9.9.9-not-the-one-in-the-document";

const CHILD_RECORD: &str = "child.json";

const LOGIN_FILE: &str = "auth.json";

const DEFAULT_CONFIG_DIR: &str = ".wiz";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arm = args
        .first()
        .cloned()
        .expect("the arm is the first argument, passed through the adapter's `args` seam");
    let report = output_file(&args)
        .expect("--json-output-file <path> must be passed by the adapter under test");
    let login = login();
    record(&report, &login);

    if !login.authenticated {
        usage();
        std::process::exit(1);
    }

    match arm.as_str() {
        "ok" => {
            banner(&args);
            write(&report, document());
        }
        "real-shape" => {
            println!("Wiz CLI v{VERSION_ON_STDOUT}, commit 0000000");
            println!("scanning {} at {DIGEST_ON_STDOUT}", image(&args));
            write(&report, REAL_SHAPE.to_string());
        }
        "library-clean" => {
            banner(&args);
            write(
                &report,
                report_with(libraries(&[]), os_packages(&DEFAULT_OS_CVES))
                    .raw()
                    .to_string(),
            );
        }
        "clean-image" => {
            banner(&args);
            write(&report, clean_document());
        }
        "no-client-version" => {
            banner(&args);
            write(&report, with_client_version(&clean_document(), None));
        }
        "blank-client-version" => {
            banner(&args);
            write(&report, with_client_version(&clean_document(), Some("  ")));
        }
        "no-scan-origin" => {
            banner(&args);
            write(&report, with_scan_origin(&clean_document(), None));
        }
        "blank-scan-origin" => {
            banner(&args);
            write(&report, with_scan_origin(&clean_document(), Some("  ")));
        }
        "library-only" => {
            banner(&args);
            write(
                &report,
                report_with(libraries(&DEFAULT_LIBRARY_CVES), os_packages(&[]))
                    .raw()
                    .to_string(),
            );
        }
        "python-library-advisory" => {
            banner(&args);
            write(
                &report,
                report_with(
                    python_libraries(&DEFAULT_LIBRARY_CVES),
                    os_packages(&DEFAULT_OS_CVES),
                )
                .raw()
                .to_string(),
            );
        }
        "no-published-fix" => {
            banner(&args);
            write(
                &report,
                report_with(unfixed_libraries(&DEFAULT_LIBRARY_CVES), os_packages(&[]))
                    .raw()
                    .to_string(),
            );
        }
        "medium-library-advisory" => {
            banner(&args);
            write(
                &report,
                report_with(
                    libraries_graded(&[DEFAULT_LIBRARY_CVES[0]], "MEDIUM"),
                    os_packages(&[]),
                )
                .raw()
                .to_string(),
            );
        }
        "two-os-advisories" => {
            banner(&args);
            write(
                &report,
                report_with(
                    libraries(&DEFAULT_LIBRARY_CVES),
                    os_packages(&[DEFAULT_OS_CVES[0], SECOND_OS_CVE]),
                )
                .raw()
                .to_string(),
            );
        }
        "second-library-still-open" => {
            banner(&args);
            write(
                &report,
                report_with(
                    libraries_carrying(&[(1, SECOND_LIBRARY_CVE)]),
                    os_packages(&DEFAULT_OS_CVES),
                )
                .raw()
                .to_string(),
            );
        }
        "two-library-advisories" => {
            banner(&args);
            write(
                &report,
                report_with(
                    libraries(&[DEFAULT_LIBRARY_CVES[0], SECOND_LIBRARY_CVE]),
                    os_packages(&DEFAULT_OS_CVES),
                )
                .raw()
                .to_string(),
            );
        }
        "exit-nonzero-with-file" => {
            banner(&args);
            write(&report, document());
            eprintln!("wizcli: policy 'default-vulnerabilities' matched 3 findings in this tenant");
            std::process::exit(3);
        }
        "exit-nonzero-no-file" => {
            banner(&args);
            eprintln!("wizcli: internal error while analysing layers");
            std::process::exit(3);
        }
        "empty-file" => {
            banner(&args);
            write(&report, String::new());
        }
        "unparseable-file" => {
            banner(&args);
            write(&report, "{\"result\": {\"libraries\": [".to_string());
        }
        "no-such-image" => {
            eprintln!(
                "wizcli: failed to inspect {}: Error response from daemon: no such image",
                image(&args)
            );
            std::process::exit(3);
        }
        "no-daemon" => {
            eprintln!(
                "wizcli: failed to inspect {}: Cannot connect to the Docker daemon at \
                 unix:///var/run/docker.sock. Is the docker daemon running?",
                image(&args)
            );
            std::process::exit(3);
        }
        "leaks-its-credential" => {
            banner(&args);
            eprintln!(
                "wizcli: client {} rejected the secret {}",
                login.client_id, login.client_secret
            );
            std::process::exit(3);
        }
        other => panic!("unknown arm {other}"),
    }
}

fn record(report: &Path, login: &Login) {
    let record = report.with_file_name(CHILD_RECORD);
    let argv: Vec<String> = std::env::args().collect();
    let env: Vec<String> = std::env::vars()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    std::fs::write(
        &record,
        serde_json::json!({
            "argv": argv,
            "env": env,
            "login": {
                "config_dir": login.config_dir.as_ref().map(|dir| dir.display().to_string()),
                "client_id": login.client_id,
                "client_secret": login.client_secret,
            },
        })
        .to_string(),
    )
    .unwrap_or_else(|source| panic!("could not write {}: {source}", record.display()));
}

struct Login {
    config_dir: Option<PathBuf>,
    authenticated: bool,
    client_id: String,
    client_secret: String,
}

fn login() -> Login {
    let config_dir = named_directory("WIZ_CONFIG_DIR")
        .or_else(|| named_directory("HOME").map(|home| home.join(DEFAULT_CONFIG_DIR)));
    let Some(directory) = config_dir else {
        return Login {
            config_dir: None,
            authenticated: false,
            client_id: String::new(),
            client_secret: String::new(),
        };
    };
    let authenticated =
        std::fs::read_dir(&directory).is_ok_and(|mut entries| entries.next().is_some());
    let written: serde_json::Value = std::fs::read_to_string(directory.join(LOGIN_FILE))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(serde_json::Value::Null);
    Login {
        config_dir: Some(directory),
        authenticated,
        client_id: written["clientId"].as_str().unwrap_or_default().to_string(),
        client_secret: written["clientSecret"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    }
}

fn named_directory(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn usage() {
    println!("               _ _   ");
    println!("__      __(_)___| (_) ");
    println!("\\ \\ /\\ / /| |_  /| | ");
    println!(" \\ V  V / | |/ / | | ");
    println!("  \\_/\\_/  |_/___||_| ");
    println!();
    println!("Usage:");
    println!("  wizcli [command]");
}

fn document() -> String {
    report_with(
        libraries(&DEFAULT_LIBRARY_CVES),
        os_packages(&DEFAULT_OS_CVES),
    )
    .raw()
    .to_string()
}

fn clean_document() -> String {
    report_with(libraries(&[]), os_packages(&[]))
        .raw()
        .to_string()
}

fn with_client_version(document: &str, version: Option<&str>) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(document).expect("a fixture document is JSON");
    let extra_info = document["extraInfo"]
        .as_object_mut()
        .expect("a fixture document records extraInfo as an object");
    match version {
        Some(version) => {
            extra_info.insert("clientVersion".to_string(), version.into());
        }
        None => {
            extra_info.remove("clientVersion");
        }
    }
    document.to_string()
}

fn with_scan_origin(document: &str, id: Option<&str>) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(document).expect("a fixture document is JSON");
    match id {
        Some(id) => document["scanOriginResource"]["id"] = id.into(),
        None => {
            document
                .as_object_mut()
                .expect("a fixture document is an object")
                .remove("scanOriginResource");
        }
    }
    document.to_string()
}

fn banner(args: &[String]) {
    println!("wizcli {FIXTURE_CLIENT_VERSION}");
    println!("scanning {} at {DIGEST_ON_STDOUT}", image(args));
}

fn output_file(args: &[String]) -> Option<PathBuf> {
    let at = args.iter().position(|arg| arg == "--json-output-file")?;
    args.get(at + 1).map(PathBuf::from)
}

fn image(args: &[String]) -> String {
    args.last().cloned().unwrap_or_default()
}

fn write(report: &PathBuf, body: String) {
    std::fs::write(report, body)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", report.display()));
}
