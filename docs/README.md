# Documentation

Documentation for the agents developing this project. Nothing here is written for a
reader outside it.

```
docs/
├── technical/
│   ├── SYSTEM.md      — what each component is
│   ├── RUNBOOKS.md    — how to run something, commands not prose
│   ├── style.md       — how to write in this repository
│   └── decisions/     — why a decision was made, one record each, append only
├── BACKLOG.md         — defects and ideas, dated, append only
├── evaluator-calibration-general.md — what a score means in this project
└── plans/             — one plan per milestone
```

A decision belongs in an ADR. A defect belongs in BACKLOG or a bean. How to run
something belongs in RUNBOOKS. What a component is belongs in SYSTEM.

An ADR carries a `Cites:` line naming the symbols it rests on, and
`scripts/check-adr-cites.sh` refuses a record whose citation resolves to nothing.
That is what makes a rename find its stale documentation.

## What is not here

The product manual and the product documents were removed. See decision 072.
