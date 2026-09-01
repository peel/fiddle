use rig_core::completion::CompletionRequest;
use rig_core::test_utils::MockCompletionModel;

pub const FENCE: char = '`';

pub struct Quotation {
    pub fence: String,
    pub inside: String,
}

pub fn longest_run_of_fences(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for character in text.chars() {
        run = match character == FENCE {
            true => run + 1,
            false => 0,
        };
        longest = longest.max(run);
    }
    longest
}

fn strings_in(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => into.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                strings_in(item, into);
            }
        }
        serde_json::Value::Object(fields) => {
            for field in fields.values() {
                strings_in(field, into);
            }
        }
        _ => {}
    }
}

pub fn texts_of(request: &CompletionRequest) -> Vec<String> {
    let mut texts: Vec<String> = request.preamble.clone().into_iter().collect();
    strings_in(
        &serde_json::to_value(&request.chat_history)
            .expect("the messages the model received serialize"),
        &mut texts,
    );
    texts
}

pub fn carried_by(requests: &[CompletionRequest]) -> Vec<Vec<String>> {
    requests.iter().map(texts_of).collect()
}

pub fn what_each_request_carried(model: &MockCompletionModel) -> Vec<Vec<String>> {
    carried_by(&model.requests())
}

pub fn carrying<'a>(planted: &str, texts: &'a [String]) -> Vec<&'a String> {
    texts.iter().filter(|text| text.contains(planted)).collect()
}

pub fn quotation_in(sent: &str) -> Quotation {
    let lines: Vec<&str> = sent.lines().collect();
    let fences: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| {
            let trimmed = line.trim();
            match !trimmed.is_empty() && trimmed.chars().all(|character| character == FENCE) {
                true => Some((at, trimmed.chars().count())),
                false => None,
            }
        })
        .collect();
    let longest = fences
        .iter()
        .map(|(_, length)| *length)
        .max()
        .unwrap_or_else(|| panic!("no line of this text is a fence line: {sent}"));
    let outermost: Vec<usize> = fences
        .iter()
        .filter(|(_, length)| *length == longest)
        .map(|(at, _)| *at)
        .collect();
    assert_eq!(
        outermost.len(),
        2,
        "a quotation is opened and closed by two fence lines of its longest run, and {} lines \
         of this text are {longest} fences long: {sent}",
        outermost.len()
    );
    Quotation {
        fence: FENCE.to_string().repeat(longest),
        inside: lines[outermost[0] + 1..outermost[1]].join("\n"),
    }
}
