use crate::agent::ToolReceipts;
use std::collections::BTreeMap;
use std::path::Path;

pub const BODY_LIMIT: usize = 65_536;

const ARGS_SHOWN: usize = 120;
const TOOL_RECORD: &str = "tool";

pub const RECEIPTS_CARRY_NO_ARGUMENTS: &str =
    "This log was built from tool receipts, which record no arguments, so it cannot say \
     which call repeated or which search matched nothing. Set `FIDDLE_TRANSCRIPT=1` to \
     record a transcript and this log carries that detail.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Transcript,
    Receipts,
}

impl Source {
    fn spelled(self) -> &'static str {
        match self {
            Source::Transcript => "the transcript",
            Source::Receipts => "tool receipts",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Call {
    pub tool: String,
    pub args: Option<String>,
    pub duration_ms: u64,
    pub empty: bool,
}

#[derive(Clone, Debug)]
pub struct WorkLog {
    pub source: Source,
    pub calls: Vec<Call>,
    pub turns: u64,
}

fn matched_nothing(result: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()
        .and_then(|value| {
            value
                .get("matches")
                .map(|matches| matches.as_array().is_some_and(|rows| rows.is_empty()))
        })
        .unwrap_or(false)
}

pub fn of_transcript(path: &Path) -> Option<WorkLog> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut calls = Vec::new();
    let mut turns = 0;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(turn) = record.get("turn").and_then(serde_json::Value::as_u64) {
            turns = turns.max(turn);
        }
        if record.get("record").and_then(serde_json::Value::as_str) != Some(TOOL_RECORD) {
            continue;
        }
        let Some(tool) = record.get("tool").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if tool == crate::agent::transcript::WITHHELD {
            return None;
        }
        let result = record
            .get("result")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        calls.push(Call {
            tool: tool.to_string(),
            args: record
                .get("args")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            duration_ms: record
                .get("duration_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            empty: matched_nothing(result),
        });
    }
    match calls.is_empty() {
        true => None,
        false => Some(WorkLog {
            source: Source::Transcript,
            calls,
            turns,
        }),
    }
}

pub fn of_receipts(receipts: &ToolReceipts) -> Option<WorkLog> {
    match receipts.calls.is_empty() {
        true => None,
        false => Some(WorkLog {
            source: Source::Receipts,
            turns: 0,
            calls: receipts
                .calls
                .iter()
                .map(|receipt| Call {
                    tool: receipt.tool.clone(),
                    args: None,
                    duration_ms: receipt.duration_ms,
                    empty: false,
                })
                .collect(),
        }),
    }
}

fn one_line(args: &str) -> String {
    let flat: String = args
        .chars()
        .map(|c| match c.is_control() {
            true => ' ',
            false => c,
        })
        .collect();
    let trimmed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut = match trimmed.char_indices().nth(ARGS_SHOWN) {
        Some((at, _)) => format!("{}…", &trimmed[..at]),
        None => trimmed,
    };
    cut.replace('|', "\\|")
}

struct PerTool {
    calls: usize,
    distinct: usize,
    repeated: usize,
    empty: usize,
    duration_ms: u64,
}

impl WorkLog {
    fn per_tool(&self) -> BTreeMap<&str, PerTool> {
        let mut out: BTreeMap<&str, PerTool> = BTreeMap::new();
        for (tool, group) in self.grouped() {
            let entry = out.entry(tool).or_insert(PerTool {
                calls: 0,
                distinct: 0,
                repeated: 0,
                empty: 0,
                duration_ms: 0,
            });
            for (_, count, empty, ms) in group {
                entry.calls += count;
                entry.distinct += 1;
                entry.repeated += count.saturating_sub(1);
                entry.empty += empty;
                entry.duration_ms += ms;
            }
        }
        out
    }

    #[allow(clippy::type_complexity)]
    fn grouped(&self) -> BTreeMap<&str, Vec<(Option<&str>, usize, usize, u64)>> {
        let mut keyed: BTreeMap<(&str, Option<&str>), (usize, usize, u64)> = BTreeMap::new();
        for call in &self.calls {
            let entry = keyed
                .entry((call.tool.as_str(), call.args.as_deref()))
                .or_insert((0, 0, 0));
            entry.0 += 1;
            entry.1 += usize::from(call.empty);
            entry.2 += call.duration_ms;
        }
        let mut out: BTreeMap<&str, Vec<(Option<&str>, usize, usize, u64)>> = BTreeMap::new();
        for ((tool, args), (count, empty, ms)) in keyed {
            out.entry(tool).or_default().push((args, count, empty, ms));
        }
        out
    }

    fn repeats(&self) -> Vec<(usize, &str, &str)> {
        let mut rows: Vec<(usize, &str, &str)> = self
            .grouped()
            .into_iter()
            .flat_map(|(tool, group)| {
                group
                    .into_iter()
                    .filter_map(move |(args, count, _, _)| match (count > 1, args) {
                        (true, Some(args)) => Some((count, tool, args)),
                        _ => None,
                    })
            })
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(b.2)));
        rows
    }

    pub fn rendered(&self, budget: usize) -> Option<String> {
        let spent: u64 = self.calls.iter().map(|call| call.duration_ms).sum();
        let mut head = String::from("<details><summary>Log</summary>\n\n");
        head.push_str(&match self.turns {
            0 => format!(
                "{} tool calls, {:.1}s in tools, from {}.\n\n",
                self.calls.len(),
                spent as f64 / 1000.0,
                self.source.spelled()
            ),
            turns => format!(
                "{} tool calls over {turns} turns, {:.1}s in tools, from {}.\n\n",
                self.calls.len(),
                spent as f64 / 1000.0,
                self.source.spelled()
            ),
        });
        match self.source {
            Source::Receipts => {
                head.push_str(RECEIPTS_CARRY_NO_ARGUMENTS);
                head.push_str("\n\n| tool | calls | ms |\n| --- | --- | --- |\n");
                for (tool, per) in self.per_tool() {
                    head.push_str(&format!(
                        "| {tool} | {} | {} |\n",
                        per.calls, per.duration_ms
                    ));
                }
            }
            Source::Transcript => {
                head.push_str(
                    "| tool | calls | distinct | repeated | matched nothing | ms |\n\
                     | --- | --- | --- | --- | --- | --- |\n",
                );
                for (tool, per) in self.per_tool() {
                    head.push_str(&format!(
                        "| {tool} | {} | {} | {} | {} | {} |\n",
                        per.calls, per.distinct, per.repeated, per.empty, per.duration_ms
                    ));
                }
            }
        }

        let tail = "\n</details>";
        let repeats = self.repeats();
        if !repeats.is_empty() {
            head.push_str("\nThe calls this run made more than once, most repeated first:\n\n");
            head.push_str("| n | tool | args |\n| --- | --- | --- |\n");
            let mut listed = 0;
            let mut rows = String::new();
            for (count, tool, args) in &repeats {
                let row = format!("| {count} | {tool} | `{}` |\n", one_line(args));
                let note_room = 96;
                if head.len() + rows.len() + row.len() + tail.len() + note_room > budget {
                    break;
                }
                rows.push_str(&row);
                listed += 1;
            }
            head.push_str(&rows);
            if listed < repeats.len() {
                head.push_str(&format!(
                    "\n{} of {} repeated calls are not listed here; the transcript holds them all.\n",
                    repeats.len() - listed,
                    repeats.len()
                ));
            }
        }
        head.push_str(tail);
        match head.len() <= budget {
            true => Some(head),
            false => None,
        }
    }
}

pub fn body_carrying(body: &str, log: Option<&WorkLog>) -> String {
    let Some(log) = log else {
        return body.to_string();
    };
    let base = format!("{}\n\n", body.trim_end());
    match log.rendered(BODY_LIMIT.saturating_sub(base.len())) {
        Some(rendered) => format!("{base}{rendered}"),
        None => body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ToolReceipt, ToolReceipts};

    fn record(turn: u64, tool: &str, args: &str, result: &str, ms: u64) -> String {
        serde_json::json!({
            "record": "tool",
            "turn": turn,
            "duration_ms": ms,
            "tool": tool,
            "args": args,
            "result": result,
        })
        .to_string()
    }

    fn written(lines: &[String]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().expect("a directory");
        let path = dir.path().join("a-run.jsonl");
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("a transcript");
        (dir, path)
    }

    const EMPTY: &str = r#"{"matches":[],"withheld":"nothing matched"}"#;
    const HIT: &str = r#"{"matches":["src/lib.rs:1"]}"#;

    #[test]
    fn a_transcript_yields_the_calls_the_agent_made() {
        let (_dir, path) = written(&[
            record(1, "search_files", r#"{"text":"X"}"#, EMPTY, 10),
            record(2, "search_files", r#"{"text":"X"}"#, EMPTY, 12),
            record(3, "read_file", r#"{"path":"src/lib.rs"}"#, HIT, 5),
        ]);
        let log = of_transcript(&path).expect("a transcript with tool records yields a log");
        assert_eq!(log.source, Source::Transcript);
        assert_eq!(log.calls.len(), 3);
        assert_eq!(log.turns, 3);
        assert_eq!(
            log.calls.iter().filter(|call| call.empty).count(),
            2,
            "a result whose matches array is empty is a search that matched nothing"
        );
    }

    #[test]
    fn a_withheld_transcript_is_refused_rather_than_summarised() {
        let (_dir, path) = written(&[record(
            1,
            crate::agent::transcript::WITHHELD,
            crate::agent::transcript::WITHHELD,
            crate::agent::transcript::WITHHELD,
            10,
        )]);
        assert!(
            of_transcript(&path).is_none(),
            "a run holding no credential withholds every text field, so the transcript \
             names no tool. A log built from it would present the withholding notice as \
             a tool name, which is an answer in the shape of a complete one."
        );
    }

    #[test]
    fn a_transcript_with_no_tool_records_yields_no_log() {
        let (_dir, path) = written(&[r#"{"record":"sent","turn":1}"#.to_string()]);
        assert!(of_transcript(&path).is_none());
        assert!(
            of_transcript(std::path::Path::new("/nowhere/at/all.jsonl")).is_none(),
            "an absent transcript is no log, and never a panic"
        );
    }

    #[test]
    fn the_rendered_log_groups_repeats_and_names_the_count() {
        let mut lines = Vec::new();
        for turn in 1..=46 {
            lines.push(record(turn, "search_files", r#"{"text":"X"}"#, EMPTY, 10));
        }
        lines.push(record(47, "edit_file", r#"{"path":"a"}"#, HIT, 3));
        let (_dir, path) = written(&lines);
        let log = of_transcript(&path).expect("a log");
        let rendered = log.rendered(BODY_LIMIT).expect("a log that fits");

        assert!(rendered.starts_with("<details><summary>Log</summary>"));
        assert!(rendered.ends_with("</details>"));
        assert!(
            rendered.contains("| search_files | 46 | 1 | 45 | 46 |"),
            "the per-tool row carries calls, distinct, repeated and matched-nothing: \
             {rendered}"
        );
        assert!(
            rendered.contains("| 46 | search_files |"),
            "the repeated call is listed with its count: {rendered}"
        );
        assert!(
            !rendered.contains("| 1 | edit_file |"),
            "a call made once is not a repeat: {rendered}"
        );
    }

    #[test]
    fn receipts_say_they_carry_no_arguments_and_name_the_switch() {
        let receipts = ToolReceipts {
            calls: vec![
                ToolReceipt {
                    tool: "search_files".to_string(),
                    outcome: "ok",
                    duration_ms: 10,
                },
                ToolReceipt {
                    tool: "search_files".to_string(),
                    outcome: "ok",
                    duration_ms: 12,
                },
            ],
        };
        let log = of_receipts(&receipts).expect("receipts yield a log");
        assert_eq!(log.source, Source::Receipts);
        let rendered = log.rendered(BODY_LIMIT).expect("a log that fits");
        assert!(
            rendered.contains("FIDDLE_TRANSCRIPT"),
            "a receipts-only log must name the switch that would carry arguments: {rendered}"
        );
        assert!(
            rendered.contains("| search_files | 2 |"),
            "the counts it does have are reported: {rendered}"
        );
        assert!(
            !rendered.contains("repeated first"),
            "receipts carry no arguments, so no repeat can be claimed from them: {rendered}"
        );
    }

    #[test]
    fn no_log_leaves_the_body_as_it_was() {
        assert_eq!(body_carrying("opened by fiddle", None), "opened by fiddle");
    }

    #[test]
    fn a_body_carrying_a_log_holds_both_and_stays_inside_the_limit() {
        let (_dir, path) = written(&[record(1, "read_file", r#"{"path":"a"}"#, HIT, 5)]);
        let log = of_transcript(&path).expect("a log");
        let body = body_carrying("opened by fiddle", Some(&log));
        assert!(body.starts_with("opened by fiddle"));
        assert!(body.contains("<details><summary>Log</summary>"));
        assert!(body.len() <= BODY_LIMIT);
    }

    #[test]
    fn a_log_too_large_for_the_body_drops_rows_and_names_how_many() {
        let mut lines = Vec::new();
        let mut turn = 0;
        for distinct in 0..4000 {
            for _ in 0..2 {
                turn += 1;
                lines.push(record(
                    turn,
                    "search_files",
                    &format!(r#"{{"text":"a query long enough to cost real bytes {distinct}"}}"#),
                    EMPTY,
                    1,
                ));
            }
        }
        let (_dir, path) = written(&lines);
        let log = of_transcript(&path).expect("a log");
        let body = body_carrying("opened by fiddle", Some(&log));
        assert!(
            body.len() <= BODY_LIMIT,
            "the body must fit the forge's limit, and it is {} bytes",
            body.len()
        );
        assert!(
            body.contains("of 4000 repeated calls are not listed here"),
            "a log that dropped rows must name the denominator rather than trailing off: \
             {}",
            &body[body.len().saturating_sub(400)..]
        );
    }
}
