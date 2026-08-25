use fiddle_core::EffectName;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const WORKFLOW_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Step {
    Agent {
        prompt: PathBuf,
        max_turns: u32,
    },
    Check {
        program: String,
        args: Vec<String>,
        timeout_secs: u64,
    },
    Effect {
        name: EffectName,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowFile {
    pub version: u32,
    pub name: String,
    pub stage: String,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workflow {
    name: String,
    stage: String,
    steps: Vec<Step>,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkflowError {
    #[error("a workflow with no step does no work")]
    NoSteps,

    #[error("this build reads workflow version {WORKFLOW_VERSION}, and the document says {0}")]
    Version(u32),
}

fn validate(steps: &[Step]) -> Result<(), WorkflowError> {
    match steps.is_empty() {
        true => Err(WorkflowError::NoSteps),
        false => Ok(()),
    }
}

impl Workflow {
    pub fn new(name: String, stage: String, steps: Vec<Step>) -> Result<Self, WorkflowError> {
        validate(&steps)?;
        Ok(Workflow { name, stage, steps })
    }

    pub fn to_file(&self) -> WorkflowFile {
        WorkflowFile {
            version: WORKFLOW_VERSION,
            name: self.name.clone(),
            stage: self.stage.clone(),
            steps: self.steps.clone(),
        }
    }
}

impl TryFrom<WorkflowFile> for Workflow {
    type Error = WorkflowError;

    fn try_from(file: WorkflowFile) -> Result<Self, WorkflowError> {
        match file.version == WORKFLOW_VERSION {
            true => Workflow::new(file.name, file.stage, file.steps),
            false => Err(WorkflowError::Version(file.version)),
        }
    }
}
