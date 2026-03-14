# Orchestrate: Parallel Sessions via Event Log Elimination

## Problem

The orchestrate skill uses a single `.claude/orchestrate-events.log` file for progress tracking. This serializes orchestrate sessions — only one can run at a time without log corruption.

## Design

Eliminate the event log entirely. Derive all progress from beans.

### Phase Tracking

Store the current phase as a tag on the epic bean: `orchestrate-phase:DEVELOP`.

The orchestrate skill sets this tag when entering each phase. The status script reads it via `beans show <epic-id>`.

Fallback derivation from child bean state (existing SETUP Step 5 logic) serves as a secondary signal for resumption:

- No child beans → DEFINE
- Child beans in `todo`/`in-progress` → DEVELOP
- All child beans `completed`/`needs-attention` → DELIVER
- Docs evolved (git log check) → DONE

### Status Pane

`orchestrate-status.sh` becomes pure beans queries:

- Phase: read `orchestrate-phase:*` tag from epic bean
- Bean list: `beans list --parent <epic-id>`
- Progress: derived from child bean status counts
- No file I/O, no shared state

Behavior per phase:

| Phase | Display |
|---|---|
| DISCOVER | Phase label only (epic may not exist yet) |
| DEFINE | Phase label, beans appear as they're created |
| DEVELOP | Full bean list with status symbols + progress bar |
| DELIVER | Phase label, all beans in completed state |

### SKILL.md Changes

- Remove `Initialize Event Log` step from SETUP
- Remove all `>> .claude/orchestrate-events.log` lines (phase transitions, activity logging)
- Remove `rm -f .claude/orchestrate-events.log` from CLEANUP
- Replace with `beans update <epic-id> --tag orchestrate-phase:<PHASE>` at each phase transition

### Parallel Sessions

Each orchestrate session operates on a different epic. Since all state lives in per-epic beans and per-epic tags, multiple sessions work without contention.

### Future Work

Individual agents (ralph workers, reviewers) update their assigned beans with progress during DEVELOP. Deferred.
