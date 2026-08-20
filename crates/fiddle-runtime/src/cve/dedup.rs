use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DedupError {
    #[error(
        "the history in {repo} cannot say what this branch already carries: {why}. \
         Fetch the whole history — `fetch-depth: 0` on actions/checkout — because \
         a truncated log names nothing, and the branch then reads as carrying no work"
    )]
    ShallowHistory { repo: String, why: String },

    #[error("`git {command}` in {repo}: {message}")]
    Git {
        repo: String,
        command: String,
        message: String,
    },
}

pub trait Spawn: Sync {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError>;
}

#[derive(Debug)]
pub struct Ran {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub struct Local;

impl Spawn for Local {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError> {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|source| DedupError::Git {
                repo: dir.display().to_string(),
                command: args.join(" "),
                message: source.to_string(),
            })?;
        Ok(Ran {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

#[derive(Debug, Default)]
pub struct FixedInCommits {
    words: BTreeSet<String>,
}

impl FixedInCommits {
    pub fn read(bodies: &str) -> Self {
        let words = bodies
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
            .filter(|word| !word.is_empty())
            .map(|word| word.to_ascii_uppercase())
            .collect();
        FixedInCommits { words }
    }

    pub fn names(&self, cve: &str) -> bool {
        self.words.contains(&cve.to_ascii_uppercase())
    }
}

pub fn commit_log_dedup(repo: &Path, base: &str) -> Result<FixedInCommits, DedupError> {
    commit_log_dedup_with(repo, base, &Local)
}

pub fn commit_log_dedup_with<S>(
    repo: &Path,
    base: &str,
    run: &S,
) -> Result<FixedInCommits, DedupError>
where
    S: Spawn + ?Sized,
{
    let git = |args: &[&str]| run.run("git", args, repo);
    let failed = |args: &[&str], ran: &Ran| DedupError::Git {
        repo: repo.display().to_string(),
        command: args.join(" "),
        message: ran.stderr.clone(),
    };

    const SHALLOW: [&str; 2] = ["rev-parse", "--is-shallow-repository"];
    let shallow = git(&SHALLOW)?;
    if !shallow.ok {
        return Err(failed(&SHALLOW, &shallow));
    }
    if shallow.stdout.trim() == "true" {
        return Err(DedupError::ShallowHistory {
            repo: repo.display().to_string(),
            why: "the clone is shallow, so commits before its graft point are absent".to_string(),
        });
    }

    let reference = format!("origin/{base}");
    let verify = ["rev-parse", "--verify", "--quiet", reference.as_str()];
    if !git(&verify)?.ok {
        return Err(DedupError::ShallowHistory {
            repo: repo.display().to_string(),
            why: format!("{reference} is not in this clone, so there is no range to read"),
        });
    }

    let range = format!("{reference}..HEAD");
    let log = ["log", "--format=%B", range.as_str()];
    let bodies = git(&log)?;
    if !bodies.ok {
        return Err(failed(&log, &bodies));
    }

    Ok(FixedInCommits::read(&bodies.stdout))
}
