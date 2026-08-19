use std::io::Write;
use std::path::{Path, PathBuf};

const FOREVER: std::time::Duration = std::time::Duration::from_secs(600);

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let dir = take_stub_dir(&mut args).expect("--stub-dir <path> must be passed through cli.args");

    let mut body_in = String::new();
    if args.iter().any(|a| a == "--input") {
        use std::io::Read;
        let _ = std::io::stdin().read_to_string(&mut body_in);
    }

    let requests = dir.join("requests");
    std::fs::create_dir_all(&requests).unwrap();
    let n = std::fs::read_dir(&requests).unwrap().count();
    let env: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
    std::fs::write(
        requests.join(format!("{n:04}.json")),
        serde_json::json!({ "argv": args, "body": body_in, "env": env }).to_string(),
    )
    .unwrap();

    if let Ok(ms) = std::fs::read_to_string(dir.join("sleep_ms")) {
        let ms: u64 = ms.trim().parse().unwrap_or(0);
        let marker = dir.join("descendant_survived_the_deadline");
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "sleep {}; : > '{}'",
                ms as f64 / 1000.0,
                marker.display()
            ))
            .spawn();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        std::fs::write(dir.join("survived_the_deadline"), "yes").unwrap();
    }

    if endpoint(&args) == Some("graphql") {
        let (status, body, mode) = graphql_answer(&dir);
        let request = graphql_request(&args).to_string();

        if let Some(ending) = mode.strip_prefix("commit_then_") {
            apply_effect(&dir, GRAPHQL_KEY, &request, &mode);
            end_without_answering(&dir, ending);
        }

        let refused = status >= 400
            || body["errors"]
                .as_array()
                .is_some_and(|errors| !errors.is_empty());
        if !refused {
            apply_effect(&dir, GRAPHQL_KEY, &request, &mode);
        }
        print!("HTTP/2.0 {status} \r\n\r\n{body}");
        std::process::exit(i32::from(refused));
    }

    let key = script_key(&args);
    if key.starts_with("GET") {
        let path = request_path(&args);
        if let Some((status, headers, body)) = comment_answer(&dir, &path) {
            print!("HTTP/2.0 {status} \r\n{headers}\r\n{body}");
            std::process::exit(if status < 400 { 0 } else { 1 });
        }
        let (status, body) = world_answer(&dir, &key, &path);
        print!("HTTP/2.0 {status} \r\n\r\n{body}");
        std::process::exit(if status < 400 { 0 } else { 1 });
    }

    let spec = std::fs::read_to_string(dir.join("script").join(&key))
        .unwrap_or_else(|_| "201 0 normal".to_string());
    let mut parts = spec.split_whitespace();
    let status: u16 = parts.next().unwrap().parse().unwrap();
    let exit: i32 = parts.next().unwrap().parse().unwrap();
    let mode = parts.next().unwrap_or("normal");

    if let Some(ending) = mode.strip_prefix("commit_then_") {
        apply_effect(&dir, &key, &body_in, mode);
        end_without_answering(&dir, ending);
    }

    if mode == "garbage" {
        let token = std::env::var("GH_TOKEN").unwrap_or_default();
        print!("this is not an HTTP response\n\nneither is this");
        eprint!("gh: could not authenticate with {token}");
        std::process::exit(exit);
    }

    if mode == "conflict" {
        open_pull_request_for(&dir, &body_in);
    }

    if status < 400 {
        apply_effect(&dir, &key, &body_in, mode);
    }
    print!(
        "HTTP/2.0 {status} \r\n{}\r\n{}",
        response_headers(mode),
        response_body(&dir, status, mode, &key)
    );
    std::process::exit(exit);
}

fn end_without_answering(dir: &Path, ending: &str) -> ! {
    match ending {
        "die" => std::process::exit(137),
        "abort" => std::process::abort(),
        "wait" => {
            std::fs::write(dir.join("landed_and_waiting"), "yes").unwrap();
            std::thread::sleep(FOREVER);
            std::process::exit(1);
        }
        other => panic!("unknown ending mode {other}"),
    }
}

fn response_headers(mode: &str) -> String {
    match mode {
        "rate_limited" => concat!(
            "Access-Control-Expose-Headers: ETag, Retry-After, X-RateLimit-Remaining\r\n",
            "Content-Type: application/json; charset=utf-8\r\n",
            "Retry-After: 60\r\n",
            "X-RateLimit-Remaining: 0\r\n",
        )
        .to_string(),
        _ => String::new(),
    }
}

fn response_body(dir: &Path, status: u16, mode: &str, key: &str) -> String {
    if mode == "echo_token" {
        let token = std::env::var("GH_TOKEN").unwrap_or_default();
        return serde_json::json!({ "message": format!("Bad credentials: {token}") }).to_string();
    }
    if mode == "answers_a_run_id" {
        return serde_json::json!({ "id": 999_999 }).to_string();
    }
    if status < 400 && is_conversation_comment_post(key) {
        return serde_json::json!({ "id": last_posted_comment_id(dir, key) }).to_string();
    }
    if status < 400 && is_pull_request_create(key) {
        let number = pull_requests(dir)
            .last()
            .and_then(|pr| pr["number"].as_u64())
            .unwrap_or_else(|| panic!("a create landed under {key} and the world holds none"));
        return serde_json::json!({ "number": number }).to_string();
    }
    match status >= 400 {
        true => serde_json::json!({ "message": format!("scripted {status}") }).to_string(),
        false => "{}".to_string(),
    }
}

fn take_stub_dir(args: &mut Vec<String>) -> Option<PathBuf> {
    let at = args.iter().position(|a| a == "--stub-dir")?;
    let dir = args.get(at + 1)?.clone();
    args.drain(at..=at + 1);
    Some(PathBuf::from(dir))
}

fn script_key(args: &[String]) -> String {
    let mut method = "GET".to_string();
    let mut path = String::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--method" | "-X" => method = it.next().cloned().unwrap_or_default(),
            "api" | "-i" | "--input" | "-" => {}
            other if other.starts_with('/') => path = other.to_string(),
            _ => {}
        }
    }
    format!(
        "{method}_{}",
        path.trim_start_matches('/')
            .replace(['/', '?', '&', '=', '%'], "_")
    )
}

fn endpoint(args: &[String]) -> Option<&str> {
    let mut rest = args.iter().skip_while(|a| a.as_str() != "api");
    rest.next()?;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--method" | "-X" | "-f" | "-F" | "--input" => {
                rest.next();
            }
            flag if flag.starts_with('-') => {}
            other => return Some(other),
        }
    }
    None
}

const GRAPHQL_KEY: &str = "POST_graphql";

fn graphql_answer(dir: &Path) -> (u16, serde_json::Value, String) {
    let counter = dir.join("graphql_calls");
    let n: usize = std::fs::read_to_string(&counter)
        .ok()
        .and_then(|seen| seen.trim().parse().ok())
        .unwrap_or(0);
    std::fs::write(&counter, (n + 1).to_string()).unwrap();

    let file = dir.join("graphql").join(format!("{n}.json"));
    let scripted = std::fs::read_to_string(&file).unwrap_or_else(|_| {
        eprintln!(
            "nothing scripted at {}; a GraphQL call is answered from its file or not at all",
            file.display()
        );
        panic!("no scripted GraphQL answer");
    });
    let scripted: serde_json::Value =
        serde_json::from_str(&scripted).expect("a scripted GraphQL answer must be JSON");
    (
        scripted["status"].as_u64().unwrap_or(200) as u16,
        scripted["body"].clone(),
        scripted["mode"].as_str().unwrap_or("normal").to_string(),
    )
}

fn graphql_request(args: &[String]) -> serde_json::Value {
    let mut fields = serde_json::Map::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg != "-f" && arg != "-F" {
            continue;
        }
        if let Some((name, value)) = it.next().and_then(|field| field.split_once('=')) {
            fields.insert(name.to_string(), value.into());
        }
    }
    serde_json::Value::Object(fields)
}

fn request_path(args: &[String]) -> String {
    args.iter()
        .find(|a| a.starts_with('/'))
        .cloned()
        .unwrap_or_default()
}

fn query_param(path: &str, name: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&value[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn pull_requests(dir: &Path) -> Vec<serde_json::Value> {
    let seeded = read_pull_request_seed(dir);
    let created = std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|w| {
            let key = w["key"].as_str().unwrap_or_default();
            key.starts_with("POST") && key.contains("pulls")
        })
        .map(|w| {
            serde_json::from_str::<serde_json::Value>(w["body"].as_str().unwrap_or("{}"))
                .unwrap_or(serde_json::Value::Null)
        })
        .collect::<Vec<_>>();

    let all: Vec<serde_json::Value> = seeded.into_iter().chain(created).collect();
    let named: Vec<u64> = all.iter().filter_map(|pr| pr["number"].as_u64()).collect();

    let mut next = 7u64;
    all.iter()
        .map(|pr| {
            let head = pr["head"].as_str().unwrap_or_default().to_string();
            let bare = head.split_once(':').map(|(_, r)| r).unwrap_or(&head);
            let number = pr["number"].as_u64().unwrap_or_else(|| {
                while named.contains(&next) {
                    next += 1;
                }
                let mine = next;
                next += 1;
                mine
            });
            serde_json::json!({
                "number": number,
                "state": pr["state"].as_str().unwrap_or("open"),
                "title": pr["title"].as_str().unwrap_or_default(),
                "head": {
                    "label": head,
                    "ref": bare,
                    "sha": bare_repository_ref(&dir.join("remote.git"), bare),
                },
                "base": { "ref": pr["base"].as_str().unwrap_or_default() },
                "labels": labels_of(dir, number, pr),
                "body": pr["body"].clone(),
                "draft": pr["draft"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn labels_of(dir: &Path, number: u64, seeded: &serde_json::Value) -> Vec<serde_json::Value> {
    let name =
        |it: &serde_json::Value| serde_json::json!({ "name": it.as_str().unwrap_or_default() });
    let mut labels: Vec<serde_json::Value> = seeded["labels"]
        .as_array()
        .map(|it| it.iter().map(name).collect())
        .unwrap_or_default();

    let applied = format!("_issues_{number}_labels");
    for landed in std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
    {
        let key = landed["key"].as_str().unwrap_or_default();
        if !key.starts_with("POST") || !key.ends_with(&applied) {
            continue;
        }
        let body = parse(landed["body"].as_str().unwrap_or("{}"));
        if let Some(sent) = body["labels"].as_array() {
            labels.extend(sent.iter().map(name));
        }
    }
    labels
}

fn read_issue_seed(dir: &Path) -> Vec<serde_json::Value> {
    read_seed(dir, "issues_seed")
}

fn read_pull_request_seed(dir: &Path) -> Vec<serde_json::Value> {
    read_seed(dir, "pulls_seed")
}

fn read_seed(dir: &Path, name: &str) -> Vec<serde_json::Value> {
    serde_json::from_str::<Vec<serde_json::Value>>(
        &std::fs::read_to_string(dir.join(name)).unwrap_or_default(),
    )
    .unwrap_or_default()
}

fn open_pull_request_for(dir: &Path, body: &str) {
    let request: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let mut seed = read_pull_request_seed(dir);
    seed.push(serde_json::json!({
        "head": request["head"].as_str().unwrap_or_default(),
        "base": request["base"].as_str().unwrap_or_default(),
        "title": "opened by another run",
    }));
    std::fs::write(
        dir.join("pulls_seed"),
        serde_json::Value::Array(seed).to_string(),
    )
    .unwrap();
}

fn apply_effect(dir: &Path, key: &str, body: &str, mode: &str) {
    if !key.starts_with("POST") && !key.starts_with("DELETE") && !key.starts_with("PATCH") {
        return;
    }
    let mut landed = serde_json::json!({ "key": key, "body": body, "mode": mode });
    if is_conversation_comment_post(key) {
        landed["comment_id"] = serde_json::json!(next_comment_id(dir));
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("world"))
        .unwrap();
    writeln!(f, "{landed}").unwrap();
}

fn is_conversation_comment_post(key: &str) -> bool {
    key.starts_with("POST_repos_") && key.contains("_issues_") && key.ends_with("_comments")
}

fn is_pull_request_create(key: &str) -> bool {
    key.starts_with("POST_repos_") && key.ends_with("_pulls")
}

fn next_comment_id(dir: &Path) -> u64 {
    let highest = listed_conversation_comments(dir)
        .iter()
        .filter_map(|comment| comment["id"].as_u64())
        .max();
    match highest {
        Some(highest) => (highest + 1).max(FIRST_POSTED_COMMENT),
        None => FIRST_POSTED_COMMENT,
    }
}

fn last_posted_comment_id(dir: &Path, key: &str) -> u64 {
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(key))
        .filter_map(|landed| landed["comment_id"].as_u64())
        .next_back()
        .unwrap_or_else(|| {
            panic!("no comment has landed under {key}, so no create can be answered about one")
        })
}

fn world_answer(dir: &Path, key: &str, path: &str) -> (u16, String) {
    let world = std::fs::read_to_string(dir.join("world")).unwrap_or_default();
    let landed: Vec<serde_json::Value> = world
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let landed_key = |w: &serde_json::Value, needle: &str| {
        w["key"].as_str().unwrap_or_default().contains(needle)
    };

    if key.starts_with("GET_repos") && key.contains("git_ref_heads") {
        let branch = key
            .split("git_ref_heads_")
            .nth(1)
            .unwrap_or_default()
            .replace('_', "/");
        return match bare_repository_ref(&dir.join("remote.git"), &branch) {
            Some(sha) => (200, format!(r#"{{"object":{{"sha":"{sha}"}}}}"#)),
            None => (404, r#"{"message":"Not Found"}"#.to_string()),
        };
    }
    if let Some(answer) = pull_request_by_number(dir, path) {
        return answer;
    }
    if key.starts_with("GET_repos") && key.contains("_issues") && !key.contains("comments") {
        let unfiltered = dir.join("issues_unfiltered").exists();
        let asked: Vec<String> = query_param(path, "labels")
            .unwrap_or_default()
            .split(',')
            .filter(|it| !it.is_empty())
            .map(str::to_string)
            .collect();

        let pulls = pull_requests(dir).into_iter().map(|pr| {
            let number = pr["number"].as_u64().unwrap_or_default();
            serde_json::json!({
                "number": number,
                "state": pr["state"],
                "title": pr["title"],
                "labels": pr["labels"],
                "pull_request": { "url": format!("/repos/o/r/pulls/{number}") },
            })
        });
        let issues = read_issue_seed(dir).into_iter().map(|issue| {
            serde_json::json!({
                "number": issue["number"],
                "state": "open",
                "title": issue["title"],
                "labels": issue["labels"]
                    .as_array()
                    .map(|it| it
                        .iter()
                        .map(|name| serde_json::json!({ "name": name }))
                        .collect::<Vec<_>>())
                    .unwrap_or_default(),
            })
        });

        let matches: Vec<_> = pulls
            .chain(issues)
            .filter(|it| {
                unfiltered
                    || (asked.iter().all(|wanted| {
                        it["labels"].as_array().is_some_and(|held| {
                            held.iter().any(|l| l["name"].as_str() == Some(wanted))
                        })
                    }) && match query_param(path, "state") {
                        Some(state) => it["state"].as_str() == Some(state.as_str()),
                        None => true,
                    })
            })
            .collect();
        return (200, serde_json::Value::Array(matches).to_string());
    }
    if key.starts_with("GET_repos") && key.contains("pulls") {
        let unfiltered = dir.join("pulls_unfiltered").exists();
        let matches: Vec<_> = pull_requests(dir)
            .into_iter()
            .filter(|pr| {
                unfiltered
                    || [
                        ("head", pr["head"]["label"].as_str()),
                        ("base", pr["base"]["ref"].as_str()),
                        ("state", pr["state"].as_str()),
                    ]
                    .into_iter()
                    .all(|(name, held)| match query_param(path, name) {
                        Some(asked) => held == Some(asked.as_str()),
                        None => true,
                    })
            })
            .collect();
        return (200, serde_json::Value::Array(matches).to_string());
    }
    if key.starts_with("GET_repos") && key.contains("commits") && key.contains("check-runs") {
        if let Some(status) = unreadable(dir, "checks_unreadable") {
            return (status, format!(r#"{{"message":"scripted {status}"}}"#));
        }
        let asked = path
            .split('?')
            .next()
            .unwrap_or_default()
            .split('/')
            .nth(5)
            .unwrap_or_default();
        let unfiltered = dir.join("checks_unfiltered").exists();
        let runs: Vec<_> = read_seed(dir, "checks_seed")
            .into_iter()
            .filter(|check| unfiltered || check["head_sha"].as_str() == Some(asked))
            .collect();
        return (
            200,
            serde_json::json!({ "total_count": runs.len(), "check_runs": runs }).to_string(),
        );
    }
    if key.starts_with("GET_repos") && key.contains("actions_workflows") {
        if let Some(status) = unreadable(dir, "runs_unreadable") {
            return (status, format!(r#"{{"message":"scripted {status}"}}"#));
        }
        if landed.iter().any(|w| landed_key(w, "dispatches")) {
            if let Some(status) = unreadable(dir, "runs_unreadable_after_a_dispatch") {
                return (status, format!(r#"{{"message":"scripted {status}"}}"#));
            }
        }
        let dispatched = landed
            .iter()
            .filter(|w| landed_key(w, "dispatches"))
            .map(|w| {
                let body = parse(w["body"].as_str().unwrap_or("{}"));
                serde_json::json!({
                    "name": format!("fiddle-{}", effect_id_in(&w["body"])),
                    "status": "queued",
                    "head_branch": body["ref"],
                })
            });
        let runs: Vec<_> = read_seed(dir, "runs_seed")
            .into_iter()
            .chain(dispatched)
            .enumerate()
            .map(|(i, run)| {
                serde_json::json!({
                    "id": 4200 + i,
                    "name": run["name"],
                    "status": run["status"].as_str().unwrap_or("queued"),
                    "head_branch": run["head_branch"],
                    "event": "workflow_dispatch",
                })
            })
            .filter(|run| {
                [
                    ("branch", run["head_branch"].as_str()),
                    ("event", run["event"].as_str()),
                ]
                .into_iter()
                .all(|(name, held)| match query_param(path, name) {
                    Some(asked) => held == Some(asked.as_str()),
                    None => true,
                })
            })
            .collect();
        return (
            200,
            serde_json::json!({ "workflow_runs": runs }).to_string(),
        );
    }
    (404, r#"{"message":"Not Found"}"#.to_string())
}

fn pull_request_by_number(dir: &Path, path: &str) -> Option<(u16, String)> {
    let bare = path.split('?').next().unwrap_or(path);
    let segments: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    let ["repos", _, _, "pulls", number] = segments.as_slice() else {
        return None;
    };
    number.parse::<u64>().ok()?;

    let file = dir.join("pulls_by_number").join(format!("{number}.json"));
    let body = match std::fs::read_to_string(&file) {
        Ok(scripted) => scripted,
        Err(_) => match pull_requests(dir)
            .into_iter()
            .find(|pr| pr["number"].as_u64().map(|it| it.to_string()).as_deref() == Some(*number))
        {
            Some(held) => held.to_string(),
            None => panic!(
                "nothing scripted at {} and no pull request numbered {number} in this \
                 world; a pull request is answered from its file, from the world, or \
                 not at all",
                file.display()
            ),
        },
    };
    Some((200, landed_transitions_applied(dir, number, body)))
}

fn landed_transitions_applied(dir: &Path, number: &str, body: String) -> String {
    let readied = {
        let node_id = parse(&body)["node_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        !node_id.is_empty() && readied_node_ids(dir).iter().any(|id| id == &node_id)
    };
    let rewritten = landed_body_rewrites(dir, number);
    if !readied && rewritten.is_none() {
        return body;
    }

    let mut pull_request = parse(&body);
    if readied {
        pull_request["draft"] = serde_json::Value::Bool(false);
    }
    if let Some(rewritten) = rewritten {
        pull_request["body"] = serde_json::Value::String(rewritten);
    }
    pull_request.to_string()
}

fn landed_body_rewrites(dir: &Path, number: &str) -> Option<String> {
    let key = format!("_pulls_{number}");
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| {
            landed["key"]
                .as_str()
                .is_some_and(|k| k.starts_with("PATCH_repos_") && k.ends_with(&key))
        })
        .filter_map(|landed| {
            parse(landed["body"].as_str().unwrap_or_default())["body"]
                .as_str()
                .map(str::to_string)
        })
        .next_back()
}

fn readied_node_ids(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(GRAPHQL_KEY))
        .filter_map(|landed| {
            let request = parse(landed["body"].as_str().unwrap_or_default());
            let id = request["id"].as_str()?.to_string();
            request["query"]
                .as_str()
                .unwrap_or_default()
                .contains("markPullRequestReadyForReview")
                .then_some(id)
        })
        .collect()
}

fn comment_answer(dir: &Path, path: &str) -> Option<(u16, String, String)> {
    let bare = path.split('?').next().unwrap_or(path);
    let segments: Vec<&str> = bare.split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        ["repos", _, _, "issues", "comments", id] => Some(comment_by_id(dir, id)),
        ["repos", _, _, "issues", _, "comments"] => Some(comment_page(dir, "issue-comments", path)),
        ["repos", _, _, "pulls", _, "comments"] => Some(comment_page(dir, "review-comments", path)),
        _ => None,
    }
}

fn comment_page(dir: &Path, collection: &str, path: &str) -> (u16, String, String) {
    if let Some(status) = unreadable(dir, &format!("{collection}-unreadable")) {
        return (
            status,
            String::new(),
            format!(r#"{{"message":"scripted {status}"}}"#),
        );
    }
    let page: u64 = query_param(path, "page")
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let bare_path = path.split('?').next().unwrap_or(path);
    let file = |k: u64| dir.join(collection).join(format!("page-{k}.json"));

    let body = std::fs::read_to_string(file(page)).unwrap_or_else(|_| {
        panic!(
            "nothing scripted at {}; a comment page is answered from its file or not at all",
            file(page).display()
        )
    });
    let link = |k: u64, rel: &str| {
        format!("<https://api.github.com{bare_path}?per_page=100&page={k}>; rel=\"{rel}\"")
    };
    let mut rels = Vec::new();
    if file(page + 1).exists() {
        rels.push(link(page + 1, "next"));
    }
    if page > 1 {
        rels.push(link(page - 1, "prev"));
        rels.push(link(1, "first"));
    }
    let scripted = std::fs::read_to_string(dir.join(collection).join(format!("page-{page}.link")));
    let headers = match scripted {
        Ok(header) => format!("Link: {}\r\n", header.trim_end_matches(['\r', '\n'])),
        Err(_) => match rels.is_empty() {
            true => String::new(),
            false => format!("Link: {}\r\n", rels.join(", ")),
        },
    };
    let last = !file(page + 1).exists();
    (
        200,
        headers,
        with_posted_comments(dir, collection, last, bare_path, body),
    )
}

const CONVERSATION: &str = "issue-comments";

const FIRST_POSTED_COMMENT: u64 = 9000;

fn with_posted_comments(
    dir: &Path,
    collection: &str,
    last_page: bool,
    bare_path: &str,
    body: String,
) -> String {
    if collection != CONVERSATION || !last_page {
        return body;
    }
    let posted = posted_comments(dir, bare_path);
    if posted.is_empty() {
        return body;
    }
    let serde_json::Value::Array(mut listed) = parse(&body) else {
        return body;
    };
    listed.extend(posted);
    serde_json::Value::Array(listed).to_string()
}

fn posted_comments(dir: &Path, bare_path: &str) -> Vec<serde_json::Value> {
    let key = format!(
        "POST_{}",
        bare_path.trim_start_matches('/').replace('/', "_")
    );
    std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|landed| landed["key"].as_str() == Some(key.as_str()))
        .map(|landed| {
            let request = parse(landed["body"].as_str().unwrap_or_default());
            let id = landed["comment_id"].as_u64().unwrap_or_else(|| {
                panic!(
                    "a comment landed under {key} with no id recorded at post time: \
                     {landed}"
                )
            });
            serde_json::json!({
                "id": id,
                "body": request["body"].as_str().unwrap_or_default(),
                "created_at": "2026-08-11T00:00:00Z",
                "updated_at": "2026-08-11T00:00:00Z",
                "author_association": "OWNER",
                "user": { "login": "fiddle[bot]", "id": 1_000_001, "type": "Bot" },
                "performed_via_github_app": serde_json::Value::Null,
            })
        })
        .collect()
}

fn comment_by_id(dir: &Path, id: &str) -> (u16, String, String) {
    let file = dir
        .join("issue-comments")
        .join("by-id")
        .join(format!("{id}.json"));
    if let Ok(body) = std::fs::read_to_string(&file) {
        return (200, String::new(), body);
    }

    let listed: Vec<serde_json::Value> = listed_conversation_comments(dir)
        .into_iter()
        .filter(|comment| {
            comment["id"]
                .as_u64()
                .is_some_and(|held| held.to_string() == id)
        })
        .collect();
    match listed.as_slice() {
        [only] => (200, String::new(), only.to_string()),
        [] => panic!(
            "nothing scripted at {}, and this world's conversation lists no comment \
             {id}: a comment is answered from its file, or from the listing, or not \
             at all",
            file.display()
        ),
        many => panic!(
            "{} comments in this world share id {id}, and a re-read is reported \
             rather than chosen from",
            many.len()
        ),
    }
}

fn listed_conversation_comments(dir: &Path) -> Vec<serde_json::Value> {
    let mut found = Vec::new();

    let pages = dir.join(CONVERSATION);
    let mut page = 1;
    while let Ok(text) = std::fs::read_to_string(pages.join(format!("page-{page}.json"))) {
        if let Ok(serde_json::Value::Array(listed)) = serde_json::from_str(&text) {
            found.extend(listed);
        }
        page += 1;
    }

    let mut conversations: Vec<String> = std::fs::read_to_string(dir.join("world"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|landed| landed["key"].as_str().map(str::to_string))
        .filter(|key| is_conversation_comment_post(key))
        .collect();
    conversations.sort();
    conversations.dedup();
    for key in conversations {
        let path = format!("/{}", key.trim_start_matches("POST_").replace('_', "/"));
        found.extend(posted_comments(dir, &path));
    }
    found
}

fn bare_repository_ref(remote: &Path, branch: &str) -> Option<String> {
    let loose = remote.join("refs").join("heads").join(branch);
    if let Ok(sha) = std::fs::read_to_string(loose) {
        return Some(sha.trim().to_string());
    }
    let packed = std::fs::read_to_string(remote.join("packed-refs")).ok()?;
    let wanted = format!("refs/heads/{branch}");
    packed.lines().find_map(|line| {
        let (sha, name) = line.split_once(' ')?;
        (name.trim() == wanted).then(|| sha.trim().to_string())
    })
}

fn effect_id_in(body: &serde_json::Value) -> String {
    parse(body.as_str().unwrap_or("{}"))["inputs"]["fiddle_effect_id"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn unreadable(dir: &Path, marker: &str) -> Option<u16> {
    std::fs::read_to_string(dir.join(marker))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn parse(body: &str) -> serde_json::Value {
    serde_json::from_str(body).unwrap_or(serde_json::Value::Null)
}
