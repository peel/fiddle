# 063 — A person outranks a check, and has to be quoted to do it

Status: accepted
Cites: HumanSaid, Followed, Followed::quoted, GroupStatus::Directed, DIRECTION_FRAME, RepairReport::direction

## Context

`github/comments.rs` could read a conversation, and only `capability/propose.rs` used it. Nothing read a comment on a CVE pull request, so a person writing "leave this one, the lint failure is in the probe file" was writing to nothing.

ADR 061 had just established that fiddle does not stand behind a tree whose checks fail. That rule has no exception for a person who knows something the check does not.

## Decision

The run reads the candidate's conversation, drops anything a bot wrote, and puts what people said in the brief under `DIRECTION_FRAME`, newest last. The brief says to follow a person over a check, and to quote the sentence followed in `direction`.

A person outranks a check. `GroupStatus::Directed { over, direction }` commits and publishes as a clean attempt does, and the record says which check it went over and whose words did it.

**The quote is verified against the conversation.** `Followed::quoted` squeezes whitespace and looks for the sentence in something a non-bot actually wrote. A `direction` that matches nothing is a protocol breach and refuses the whole attempt.

## Consequences

- Commenting on a CVE pull request now does something, which is the only reading under which asking a person to comment is honest.
- A model cannot free itself from a failing check by inventing a permission. The sentence has to exist in the conversation, and the author's name goes in the record beside it.
- Whitespace is squeezed before the search, so a sentence wrapped across lines in the comment still matches. Case is not folded, so a quote has to be the words as written.
- An empty or blank `direction` matches nothing, because a blank string is contained by every string and would otherwise match everything.
- A bot's comment is not direction. fiddle's own pull request body cannot instruct the next run.
- The authority is coarse: a verified direction lets the attempt past the first failing check, not past a named one. Narrowing it to a particular check or advisory is not done, and a person who wants one check waived waives all of them for that attempt.
- Nothing yet reads a review comment on a diff line, only the issue conversation.
