//! Fixture-backed implementations of the observation ports.
//!
//! M0 introduces no authenticated adapter: both ports read a directory named by
//! `stub.root` in the configuration, laid out as
//!
//! ```text
//! <stub.root>/work/<work-id>.json      -> {"id":"fiddle-m0-demo","status":"open"}
//! <stub.root>/changes/<work-id>.json   -> {"marker":"<correlation-key>"}
//! ```
//!
//! The point of the stubs is not the fixtures — it is that the seam is already
//! the one a real adapter will sit in. These are the first implementations of
//! [`WorkItemPort`] and [`ChangePort`], and they prove themselves against the
//! shared contract harness in [`crate::ports::contract`], not against tests of
//! their own invention.

use crate::ports::{ChangePort, WorkItemPort};
use fiddle_core::{ChangeSetState, Observation, SourceRef, WorkItemState};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The origin every source reference these ports produce is namespaced by.
///
/// Named once, so the wire contract (`stub:work/<id>.json`) and the contract
/// harness's origin assertion can never drift apart.
pub const STUB_ORIGIN: &str = "stub";

/// Observes work items from `<root>/work/<work-id>.json`.
pub struct StubWorkItemPort {
    root: PathBuf,
}

impl StubWorkItemPort {
    /// A port reading the fixture directory at `root`.
    ///
    /// The directory is not checked here: whether it exists is an *observation*
    /// about the world, reported by `observe`, not a construction error. A port
    /// that refused to be built over a missing root would make an unobservable
    /// source indistinguishable from a misconfigured one.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        StubWorkItemPort { root: root.into() }
    }
}

impl WorkItemPort for StubWorkItemPort {
    fn observe(&self, work_id: &str) -> Observation<WorkItemState> {
        let rel = format!("work/{work_id}.json");
        let source = SourceRef(format!("{STUB_ORIGIN}:{rel}"));
        match read_fixture(&self.root, &rel) {
            Ok(text) => parse(&text, source),
            Err(reason) => Observation::Unavailable { source, reason },
        }
    }
}

/// Observes change sets from `<root>/changes/<work-id>.json`.
pub struct StubChangePort {
    root: PathBuf,
}

impl StubChangePort {
    /// A port reading the fixture directory at `root`. See
    /// [`StubWorkItemPort::new`] for why the root is not validated here.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        StubChangePort { root: root.into() }
    }
}

impl ChangePort for StubChangePort {
    fn observe(&self, work_id: &str) -> Observation<ChangeSetState> {
        let rel = format!("changes/{work_id}.json");
        let source = SourceRef(format!("{STUB_ORIGIN}:{rel}"));
        match read_fixture(&self.root, &rel) {
            Ok(text) => parse(&text, source),
            // A run that has not executed yet finds no change set recorded, and
            // that *is* an observation — but only if the fixture root itself
            // was readable. `read_fixture` has already established that;
            // reaching this arm means the root answered and the change set
            // simply is not there.
            Err(reason) if reason == NOT_RECORDED => Observation::Available {
                value: ChangeSetState { marker: None },
                source,
                revision: None,
            },
            Err(reason) => Observation::Unavailable { source, reason },
        }
    }
}

/// The sentinel `read_fixture` reports when the root is readable but the
/// individual fixture is absent. Only [`StubChangePort`] treats that as an
/// observation; for a work item, an absent fixture is an unobservable source.
const NOT_RECORDED: &str = "stub source not recorded";

/// Read `<root>/<rel>`, distinguishing "this fixture is not recorded" from
/// "the fixture root could not be read at all".
///
/// The distinction is the whole reason this is a function rather than a call to
/// `read_to_string`: a missing file under a readable root is a fact about the
/// world, while a missing root is a failure to observe it, and collapsing the
/// two would let a mistyped `stub.root` masquerade as an empty project.
fn read_fixture(root: &Path, rel: &str) -> Result<String, String> {
    match std::fs::read_to_string(root.join(rel)) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == ErrorKind::NotFound => match std::fs::metadata(root) {
            Ok(meta) if meta.is_dir() => Err(NOT_RECORDED.to_string()),
            Ok(_) => Err(format!(
                "stub root unreadable: {} is not a directory",
                root.display()
            )),
            Err(error) => Err(format!("stub root unreadable: {error}")),
        },
        Err(error) => Err(format!("stub source unreadable: {error}")),
    }
}

/// Deserialize a fixture, reporting a parse failure as an unobservable source.
///
/// A malformed fixture is never defaulted: whatever the file was meant to say,
/// what it actually says is unreadable, and the caller is told exactly that.
fn parse<T: serde::de::DeserializeOwned>(text: &str, source: SourceRef) -> Observation<T> {
    match serde_json::from_str::<T>(text) {
        Ok(value) => Observation::Available {
            value,
            source,
            revision: None,
        },
        Err(error) => Observation::Unavailable {
            source,
            reason: format!("stub source malformed: {error}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::contract::{
        change_port_contract, work_item_port_contract, ChangeWorlds, WorkItemWorlds,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    const WORK_ID: &str = "fiddle-m0-demo";

    /// Builds the worlds the shared contract harness asks for out of temporary
    /// fixture directories. One directory per world, so no world can leave
    /// state behind that another depends on.
    struct StubWorlds {
        dirs: std::cell::RefCell<Vec<TempDir>>,
    }

    impl StubWorlds {
        fn new() -> Self {
            StubWorlds {
                dirs: std::cell::RefCell::new(Vec::new()),
            }
        }

        /// A fresh fixture root holding `work/` and `changes/`, kept alive for
        /// as long as this fixture is.
        fn root(&self) -> PathBuf {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            std::fs::create_dir_all(root.join("work")).unwrap();
            std::fs::create_dir_all(root.join("changes")).unwrap();
            self.dirs.borrow_mut().push(dir);
            root
        }

        /// A path that does not exist, so the port has nothing to read.
        fn absent_root(&self) -> PathBuf {
            self.root().join("no-such-fixture-root")
        }

        fn root_with(&self, rel: &str, contents: &str) -> PathBuf {
            let root = self.root();
            std::fs::write(root.join(rel), contents).unwrap();
            root
        }
    }

    impl WorkItemWorlds for StubWorlds {
        type Port = StubWorkItemPort;

        fn work_id(&self) -> &str {
            WORK_ID
        }

        fn origin(&self) -> &str {
            STUB_ORIGIN
        }

        fn source_absent(&self) -> Self::Port {
            StubWorkItemPort::new(self.absent_root())
        }

        fn source_malformed(&self) -> Self::Port {
            StubWorkItemPort::new(self.root_with(&format!("work/{WORK_ID}.json"), "{ not json"))
        }

        fn source_open(&self) -> Self::Port {
            StubWorkItemPort::new(self.root_with(
                &format!("work/{WORK_ID}.json"),
                &format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
            ))
        }
    }

    impl ChangeWorlds for StubWorlds {
        type Port = StubChangePort;

        fn work_id(&self) -> &str {
            WORK_ID
        }

        fn origin(&self) -> &str {
            STUB_ORIGIN
        }

        fn source_absent(&self) -> Self::Port {
            StubChangePort::new(self.absent_root())
        }

        fn source_malformed(&self) -> Self::Port {
            StubChangePort::new(self.root_with(&format!("changes/{WORK_ID}.json"), "{ not json"))
        }

        fn source_unmarked(&self) -> Self::Port {
            StubChangePort::new(self.root())
        }

        fn source_marked(&self, marker: &str) -> Self::Port {
            StubChangePort::new(self.root_with(
                &format!("changes/{WORK_ID}.json"),
                &format!(r#"{{"marker":"{marker}"}}"#),
            ))
        }
    }

    #[test]
    fn stub_work_item_port_satisfies_the_port_contract() {
        work_item_port_contract(&StubWorlds::new());
    }

    #[test]
    fn stub_change_port_satisfies_the_port_contract() {
        change_port_contract(&StubWorlds::new());
    }

    /// Beyond the shared contract: the source reference the stubs produce is
    /// part of the CLI's observable payload, so its exact spelling is pinned
    /// here rather than left to the harness's origin-prefix check.
    #[test]
    fn a_stub_source_ref_names_the_fixture_it_read() {
        let worlds = StubWorlds::new();
        let observed = WorkItemWorlds::source_open(&worlds).observe(WORK_ID);
        assert_eq!(
            observed.source().map(|s| s.0.as_str()),
            Some("stub:work/fiddle-m0-demo.json")
        );

        let observed = ChangeWorlds::source_unmarked(&worlds).observe(WORK_ID);
        assert_eq!(
            observed.source().map(|s| s.0.as_str()),
            Some("stub:changes/fiddle-m0-demo.json")
        );
    }

    /// A work item that is simply not recorded is still an unobservable work
    /// item: the invocation named work the source does not have, which is not
    /// the same as the source saying the work is empty.
    #[test]
    fn an_unrecorded_work_item_is_unavailable_rather_than_defaulted() {
        let worlds = StubWorlds::new();
        let observed = StubWorkItemPort::new(worlds.root()).observe(WORK_ID);
        assert!(observed.is_unavailable(), "got {observed:?}");
        assert_eq!(observed.value(), None);
    }
}
