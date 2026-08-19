use crate::cve::version;
use fiddle_core::{PackageType, ProjectedFinding};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rule {
    One,
    Two,
    Three,
    Four,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Target {
    Module(String),
    DockerfileBaseImage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribution {
    target: Target,
    rule: Rule,
    resolved: String,
}

impl Attribution {
    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn rule(&self) -> Rule {
        self.rule
    }

    pub fn resolved(&self) -> &str {
        &self.resolved
    }
}

#[derive(Debug, thiserror::Error)]
#[error("`{command}` could not be run: {message}")]
pub struct ResolverError {
    pub command: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AttributionError {
    #[error("no bump target; the resolver said:\n{resolved_output}")]
    NoTarget { resolved_output: String },
    #[error(transparent)]
    Resolver(#[from] ResolverError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    pub go_mod: String,
    pub go_sum: Option<String>,
}

#[async_trait::async_trait]
pub trait ModuleGraph: Sync {
    async fn list(&self, module: &str) -> Result<String, ResolverError>;

    async fn why(&self, module: &str) -> Result<String, ResolverError>;

    async fn manifest(&self) -> Result<Manifest, ResolverError>;

    async fn get(&self, module: &str, query: &str) -> Result<String, ResolverError>;

    async fn tidy(&self) -> Result<String, ResolverError>;

    async fn restore(&self, manifest: &Manifest) -> Result<(), ResolverError>;
}

pub async fn attribute<G>(
    finding: &ProjectedFinding,
    graph: &G,
) -> Result<Attribution, AttributionError>
where
    G: ModuleGraph + ?Sized,
{
    let package = finding.package.as_str();

    let mut resolved = Transcript::default();
    let listed = resolved.record(
        graph.list(package).await?,
        "go",
        ["list", "-m", "-json", package],
    );
    let why = resolved.record(
        graph.why(package).await?,
        "go",
        ["mod", "why", "-m", package],
    );

    let record = ModuleRecord::read(&listed);
    let chain = Chain::read(&why);

    match_rules(finding, &record, &chain, graph, &mut resolved).await
}

async fn match_rules<G>(
    finding: &ProjectedFinding,
    record: &Option<ModuleRecord>,
    chain: &Chain,
    graph: &G,
    resolved: &mut Transcript,
) -> Result<Attribution, AttributionError>
where
    G: ModuleGraph + ?Sized,
{
    let package = finding.package.as_str();

    if let Some(record) = record {
        if !record.indirect {
            return Ok(Attribution {
                target: Target::Module(package.to_string()),
                rule: Rule::One,
                resolved: resolved.take(),
            });
        }

        if let Some(parent) =
            the_direct_parent_in_the_chain(chain, package, graph, resolved).await?
        {
            if the_parent_carries_the_fix(&parent, finding, graph, resolved).await? {
                return Ok(Attribution {
                    target: Target::Module(parent.path),
                    rule: Rule::Two,
                    resolved: resolved.take(),
                });
            }
        }

        return Ok(Attribution {
            target: Target::Module(package.to_string()),
            rule: Rule::Three,
            resolved: resolved.take(),
        });
    }

    if finding.package_type == PackageType::Os {
        return Ok(Attribution {
            target: Target::DockerfileBaseImage,
            rule: Rule::Four,
            resolved: resolved.take(),
        });
    }

    Err(AttributionError::NoTarget {
        resolved_output: resolved.take(),
    })
}

struct Parent {
    path: String,
    version: String,
}

async fn the_direct_parent_in_the_chain<G>(
    chain: &Chain,
    package: &str,
    graph: &G,
    resolved: &mut Transcript,
) -> Result<Option<Parent>, ResolverError>
where
    G: ModuleGraph + ?Sized,
{
    for (position, hop) in chain.hops.iter().enumerate() {
        if position == 0 || hop == package {
            continue;
        }
        let listed = resolved.record(graph.list(hop).await?, "go", ["list", "-m", "-json", hop]);
        match ModuleRecord::read(&listed) {
            Some(record) if !record.indirect && !record.main => {
                return Ok(Some(Parent {
                    path: hop.clone(),
                    version: record.version,
                }))
            }
            _ => continue,
        }
    }
    Ok(None)
}

async fn the_parent_carries_the_fix<G>(
    parent: &Parent,
    finding: &ProjectedFinding,
    graph: &G,
    resolved: &mut Transcript,
) -> Result<bool, ResolverError>
where
    G: ModuleGraph + ?Sized,
{
    let package = finding.package.as_str();
    let (Some(fixed), Some(minor)) = (
        finding.fixed_version.as_deref(),
        its_own_minor(&parent.version),
    ) else {
        return Ok(false);
    };

    let before = graph.manifest().await?;
    let target = format!("{}@{minor}", parent.path);
    resolved.record(
        graph.get(&parent.path, &minor).await?,
        "go",
        ["get", target.as_str()],
    );
    resolved.record(graph.tidy().await?, "go", ["mod", "tidy"]);
    let listed = resolved.record(
        graph.list(package).await?,
        "go",
        ["list", "-m", "-json", package],
    );

    let carried =
        ModuleRecord::read(&listed).is_some_and(|record| version::at_least(&record.version, fixed));
    if !carried {
        graph.restore(&before).await?;
    }
    Ok(carried)
}

fn its_own_minor(version: &str) -> Option<String> {
    let mut components = version.split('.');
    let major = components.next()?;
    let minor = components.next()?;
    match major
        .strip_prefix('v')
        .unwrap_or(major)
        .parse::<u64>()
        .is_ok()
        && minor.parse::<u64>().is_ok()
    {
        true => Some(format!("{major}.{minor}")),
        false => None,
    }
}

#[derive(Debug, serde::Deserialize)]
struct ModuleRecord {
    #[serde(default, rename = "Indirect")]
    indirect: bool,
    #[serde(default, rename = "Main")]
    main: bool,
    #[serde(default, rename = "Version")]
    version: String,
}

impl ModuleRecord {
    fn read(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

pub(crate) fn shipped_version(listed: &str) -> Option<String> {
    ModuleRecord::read(listed)
        .map(|record| record.version)
        .filter(|version| !version.is_empty())
}

#[derive(Debug, Default)]
struct Chain {
    hops: Vec<String>,
}

impl Chain {
    fn read(text: &str) -> Self {
        let hops = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('('))
            .map(str::to_string)
            .collect();
        Chain { hops }
    }
}

#[derive(Debug, Default)]
struct Transcript(String);

impl Transcript {
    fn record<'a, A>(&mut self, output: String, program: &str, args: A) -> String
    where
        A: IntoIterator<Item = &'a str>,
    {
        let args = args.into_iter().collect::<Vec<_>>().join(" ");
        self.0
            .push_str(&format!("$ {program} {args}\n{}\n", output.trim_end()));
        output
    }

    fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}
