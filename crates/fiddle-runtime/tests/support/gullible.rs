use super::quoting::{texts_of, FENCE};
use rig_core::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse,
};
use rig_core::streaming::StreamingCompletionResponse;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use std::sync::{Arc, Mutex};

const SHORTEST_FENCE: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reading {
    Absent,
    Quoted,
    Bare,
}

pub fn reading_of(text: &str, instruction: &str) -> Reading {
    if !text.contains(instruction) {
        return Reading::Absent;
    }
    let mut opened: Option<usize> = None;
    let mut quoted = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let run = trimmed.chars().count();
        let closes = run >= SHORTEST_FENCE && trimmed.chars().all(|character| character == FENCE);
        if closes {
            opened = match opened {
                None => Some(run),
                Some(open) if run >= open => None,
                Some(open) => Some(open),
            };
            continue;
        }
        if !line.contains(instruction) {
            continue;
        }
        match opened {
            Some(_) => quoted = true,
            None => return Reading::Bare,
        }
    }
    match quoted {
        true => Reading::Quoted,
        false => Reading::Bare,
    }
}

pub fn how_it_arrived(texts: &[String], instruction: &str) -> Reading {
    let mut arrival = Reading::Absent;
    for text in texts {
        match reading_of(text, instruction) {
            Reading::Bare => return Reading::Bare,
            Reading::Quoted => arrival = Reading::Quoted,
            Reading::Absent => {}
        }
    }
    arrival
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlantedWrite {
    pub path: String,
    pub contents: String,
}

#[derive(Default)]
struct Read {
    readings: Vec<Reading>,
    requests: Vec<CompletionRequest>,
    obeyed: usize,
}

#[derive(Clone)]
pub struct Gullible {
    instruction: String,
    planted: PlantedWrite,
    scripted: MockCompletionModel,
    read: Arc<Mutex<Read>>,
}

impl Gullible {
    pub fn obeying(
        instruction: &str,
        planted: PlantedWrite,
        turns: impl IntoIterator<Item = MockTurn>,
    ) -> Self {
        Gullible {
            instruction: instruction.to_string(),
            planted,
            scripted: MockCompletionModel::new(turns),
            read: Arc::new(Mutex::new(Read::default())),
        }
    }

    pub fn readings(&self) -> Vec<Reading> {
        self.held().readings.clone()
    }

    pub fn requests(&self) -> Vec<CompletionRequest> {
        self.held().requests.clone()
    }

    pub fn times_it_obeyed(&self) -> usize {
        self.held().obeyed
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Read> {
        self.read
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn taking(&self, request: &CompletionRequest) -> Reading {
        let arrival = how_it_arrived(&texts_of(request), &self.instruction);
        let mut held = self.held();
        held.readings.push(arrival);
        held.requests.push(request.clone());
        arrival
    }

    fn obeys(&self, arrival: Reading) -> bool {
        let mut held = self.held();
        match arrival == Reading::Bare && held.obeyed == 0 {
            true => {
                held.obeyed += 1;
                true
            }
            false => false,
        }
    }

    fn doing_as_it_was_told(&self) -> MockCompletionModel {
        MockCompletionModel::new([MockTurn::tool_call(
            "obeyed",
            "write_file",
            serde_json::json!({
                "path": self.planted.path,
                "contents": self.planted.contents,
            }),
        )])
    }
}

impl CompletionModel for Gullible {
    type Response = <MockCompletionModel as CompletionModel>::Response;
    type StreamingResponse = <MockCompletionModel as CompletionModel>::StreamingResponse;
    type Client = ();

    fn make(_: &Self::Client, _: impl Into<String>) -> Self {
        Gullible::obeying(
            "",
            PlantedWrite {
                path: String::new(),
                contents: String::new(),
            },
            [],
        )
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let arrival = self.taking(&request);
        match self.obeys(arrival) {
            true => self.doing_as_it_was_told().completion(request).await,
            false => self.scripted.completion(request).await,
        }
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        self.taking(&request);
        self.scripted.stream(request).await
    }
}
