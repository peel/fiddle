use std::ops::Range;

pub const START: &str = "<!-- fiddle-attempts:start -->";
pub const END: &str = "<!-- fiddle-attempts:end -->";
const LABEL: &str = "Attempts:";

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttemptsError {
    #[error(
        "the pull request body does not record an attempt count: {why}. \
         A human can edit a body, so fiddle reads the block between \
         `<!-- fiddle-attempts:start -->` and `<!-- fiddle-attempts:end -->` \
         and never assumes it. Correct the body by hand, because a run that \
         reads no count carries no bound"
    )]
    Unreadable { why: String },
}

pub fn read(body: &str) -> Result<u32, AttemptsError> {
    match block(body)? {
        None => Ok(0),
        Some(found) => count(&body[found.held]),
    }
}

pub fn write(body: &str, attempts: u32) -> Result<String, AttemptsError> {
    let rendered = render(attempts);
    match block(body)? {
        Some(found) => {
            count(&body[found.held.clone()])?;
            let mut written = String::with_capacity(body.len() + rendered.len());
            written.push_str(&body[..found.at.start]);
            written.push_str(&rendered);
            written.push_str(&body[found.at.end..]);
            Ok(written)
        }
        None if body.is_empty() => Ok(rendered),
        None => {
            let mut written = String::with_capacity(body.len() + rendered.len() + 2);
            written.push_str(body);
            while !written.ends_with("\n\n") {
                written.push('\n');
            }
            written.push_str(&rendered);
            Ok(written)
        }
    }
}

struct Block {
    at: Range<usize>,
    held: Range<usize>,
}

fn render(attempts: u32) -> String {
    format!("{START}\n{LABEL} {attempts}\n{END}")
}

fn block(body: &str) -> Result<Option<Block>, AttemptsError> {
    let starts = offsets(body, START);
    let ends = offsets(body, END);
    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([start], [end]) if *end >= start + START.len() => Ok(Some(Block {
            at: *start..end + END.len(),
            held: start + START.len()..*end,
        })),
        ([_], [_]) => Err(unreadable("the end marker comes before the start marker")),
        ([], _) => Err(unreadable(
            "the body holds an end marker and no start marker",
        )),
        (_, []) => Err(unreadable(
            "the body holds a start marker and no end marker",
        )),
        (starts, ends) => Err(unreadable(&format!(
            "the body holds {} start markers and {} end markers, which name no \
             single count",
            starts.len(),
            ends.len()
        ))),
    }
}

fn offsets(body: &str, marker: &str) -> Vec<usize> {
    body.match_indices(marker).map(|(at, _)| at).collect()
}

fn count(held: &str) -> Result<u32, AttemptsError> {
    let line = held.trim();
    let after = line.strip_prefix(LABEL).ok_or_else(|| {
        unreadable(&format!(
            "the block holds {line:?}, which does not begin with {LABEL:?}"
        ))
    })?;
    let digits = after.trim();
    digits.parse().map_err(|_| {
        unreadable(&format!(
            "the block holds {line:?}, and {digits:?} is not a number"
        ))
    })
}

fn unreadable(why: &str) -> AttemptsError {
    AttemptsError::Unreadable {
        why: why.to_string(),
    }
}
