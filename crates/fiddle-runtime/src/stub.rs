use crate::ports::{ChangePort, WorkItemPort};
use fiddle_core::{ChangeSetState, Observation, SourceRef, WorkItemState};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const STUB_ORIGIN: &str = "stub";

pub struct StubWorkItemPort {
    root: PathBuf,
}

impl StubWorkItemPort {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        StubWorkItemPort { root: root.into() }
    }
}

#[async_trait::async_trait]
impl WorkItemPort for StubWorkItemPort {
    async fn observe(&self, work_id: &str) -> Observation<WorkItemState> {
        let rel = format!("work/{work_id}.json");
        let source = SourceRef(format!("{STUB_ORIGIN}:{rel}"));
        match read_fixture(&self.root, &rel) {
            Ok(text) => parse(&text, source),
            Err(reason) => Observation::Unavailable { source, reason },
        }
    }
}

pub struct StubChangePort {
    root: PathBuf,
}

impl StubChangePort {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        StubChangePort { root: root.into() }
    }
}

#[async_trait::async_trait]
impl ChangePort for StubChangePort {
    async fn observe(&self, work_id: &str) -> Observation<ChangeSetState> {
        let rel = format!("changes/{work_id}.json");
        let source = SourceRef(format!("{STUB_ORIGIN}:{rel}"));
        match read_fixture(&self.root, &rel) {
            Ok(text) => parse(&text, source),
            Err(reason) if reason == NOT_RECORDED => Observation::Available {
                value: ChangeSetState { marker: None },
                source,
                revision: None,
            },
            Err(reason) => Observation::Unavailable { source, reason },
        }
    }
}

const NOT_RECORDED: &str = "stub source not recorded";

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

    struct StubWorlds {
        dirs: std::cell::RefCell<Vec<TempDir>>,
    }

    impl StubWorlds {
        fn new() -> Self {
            StubWorlds {
                dirs: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn root(&self) -> PathBuf {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            std::fs::create_dir_all(root.join("work")).unwrap();
            std::fs::create_dir_all(root.join("changes")).unwrap();
            self.dirs.borrow_mut().push(dir);
            root
        }

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

    #[tokio::test]
    async fn stub_work_item_port_satisfies_the_port_contract() {
        work_item_port_contract(&StubWorlds::new()).await;
    }

    #[tokio::test]
    async fn stub_change_port_satisfies_the_port_contract() {
        change_port_contract(&StubWorlds::new()).await;
    }

    #[tokio::test]
    async fn a_stub_source_ref_names_the_fixture_it_read() {
        let worlds = StubWorlds::new();
        let observed = WorkItemWorlds::source_open(&worlds).observe(WORK_ID).await;
        assert_eq!(
            observed.source().map(|s| s.0.as_str()),
            Some("stub:work/fiddle-m0-demo.json")
        );

        let observed = ChangeWorlds::source_unmarked(&worlds).observe(WORK_ID).await;
        assert_eq!(
            observed.source().map(|s| s.0.as_str()),
            Some("stub:changes/fiddle-m0-demo.json")
        );
    }

    #[tokio::test]
    async fn an_unrecorded_work_item_is_unavailable_rather_than_defaulted() {
        let worlds = StubWorlds::new();
        let observed = StubWorkItemPort::new(worlds.root()).observe(WORK_ID).await;
        assert!(observed.is_unavailable(), "got {observed:?}");
        assert_eq!(observed.value(), None);
    }
}
