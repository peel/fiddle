# NNN — Title

Date: YYYY-MM-DD
Status: proposed | accepted
Cites: symbol_name, path/to/file.rs, script.sh

## Context

Three sentences or fewer. Say what prompted the decision. Name the constraint that bound it.

## Decision

Three sentences or fewer, imperative. Say what the project does now.

## Consequences

Five bullets or fewer, two lines each. One bullet names what the decision gave up.

## The slots

`Status` takes one of these forms. A decision that another decision changed says so on this line.

```
Status: proposed
Status: accepted
Status: superseded by NNN
Status: accepted; partially superseded by NNN
Status: accepted; amended by NNN
Status: accepted; amended in <milestone> by the note below
```

A decision that changes an earlier one says so on its own line under `Status`. Use one of these forms.

```
Supersedes NNN.
Partially supersedes NNN: <what the earlier decision loses>.
Amends NNN, which stands.
```

`Cites` names the symbols and files the decision's claims depend on. A reader who changes a symbol greps for it and finds every decision that asserts something about it. Name a symbol or quote a sentence. Do not cite a line number.

An amendment goes in a section under `Consequences`, titled `Amendment (<milestone>) — <what changed>`. The sections above it keep the text they had. The amendment says which part still holds.
