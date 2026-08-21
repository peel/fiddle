use async_trait::async_trait;
use std::path::PathBuf;

pub mod wizcli;

pub use wizcli::Wizcli;

#[async_trait]
pub trait Scanner {
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError>;
}

#[derive(Clone, Debug)]
pub struct ScanReport {
    pub document: serde_json::Value,
    pub scanner_version: String,
    pub image_digest: String,
}

impl ScanReport {
    pub fn findings(&self) -> Vec<&serde_json::Value> {
        ["libraries", "osPackages"]
            .iter()
            .filter_map(|array| self.document["result"][array].as_array())
            .flatten()
            .filter_map(|package| package["vulnerabilities"].as_array())
            .flatten()
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("the scanner {} could not be started: {reason}", program.display())]
    Missing { program: PathBuf, reason: String },

    #[error("the scanner produced no report ({status}): {stderr}")]
    Failed { status: String, stderr: String },

    #[error("the scanner wrote an empty report to {}", path.display())]
    NoOutput { path: PathBuf },

    #[error("the report at {} is not a scanner document: {reason}", path.display())]
    Unparseable { path: PathBuf, reason: String },

    #[error("no image {image}: {stderr}")]
    ImageAbsent { image: String, stderr: String },

    #[error(
        "the container daemon could not be reached, so no image was inspected; \
         start it, or point DOCKER_HOST at the one that is listening: {stderr}"
    )]
    DaemonUnreachable { stderr: String },
}

impl ScanError {
    pub fn recurrence(&self) -> crate::effect::Recurrence {
        use crate::effect::Recurrence;
        match self {
            ScanError::Missing { .. } => Recurrence::Permanent,

            ScanError::Failed { .. } => Recurrence::Correctable,

            ScanError::NoOutput { .. } | ScanError::Unparseable { .. } => Recurrence::Permanent,

            ScanError::ImageAbsent { .. } => Recurrence::Permanent,

            ScanError::DaemonUnreachable { .. } => Recurrence::Correctable,
        }
    }
}
