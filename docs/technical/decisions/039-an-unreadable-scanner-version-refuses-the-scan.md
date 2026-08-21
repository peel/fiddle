# 039 — An unreadable scanner version refuses the scan

Status: accepted
Cites: scanner/wizcli.rs::scanner_version, ScanError::Unparseable, Recurrence::Permanent, RescanVerdict::Provisional, RescanVerdict::Cleared, Repair::scanned_at, Outcome::NoArtefact, evaluate::judge, extraInfo.clientVersion

## Context

The scan document records the scanner that wrote it in `extraInfo.clientVersion`. fiddle files that version against the repair, and the rescan compares the two. That comparison is the only thing that tells a scanner upgrade from an unchanged toolchain.

`scanner_version` read the field and ended with a default value. An absent field became an empty string. Two empty strings compare equal, so an unreadable version counted as a version that did not change. `RescanVerdict::Provisional` never fired, and a rescan at an unknown version reported proof.

This is the fifth recorded instance of a plausible value that replaced a refusal.

## Decision

`scanner_version` returns a `Result`. A document that records no version is `ScanError::Unparseable`. A document that records a blank version is the same refusal, because a blank string is the same default value.

The comparison in `evaluate::judge` does not change. `Repair::scanned_at` is only ever filled from a report's version, and `Wizcli` is the only production producer of a report. So no unreadable version reaches the comparison. Two unreadable versions cannot compare equal, because neither one exists.

## Consequences

- A document that parses, carries findings, and projects correctly is now refused. One absent metadata field decides that, and the scan gives up the findings it read.
- `Unparseable` is `Recurrence::Permanent`, so nothing retries the refusal. The scan check fails as `Outcome::NoArtefact`, and the tree is rejected. The attempt becomes needs-work and its commit is reverted.
- A scanner release that renames or removes the field stops every sweep. An unattended nightly run then repairs nothing until a person reads the report. This silent stop is the cost of the decision.
- `RescanVerdict::Cleared` is now unreachable from a document that cannot say what produced it. That is the property the decision protects.
- What was given up: a sweep that continues through a scanner release fiddle did not expect.

## The alternative, if the field ever moves

Take the softer option if the field moves and the sweep must continue. Make the version an `Option`. Read an absent version as "fiddle cannot establish this" rather than as "unchanged", and let `RescanVerdict::Provisional` fire.

The argument for that option is good. `Provisional` already says that fiddle cannot prove what happened, which is what an absent version says. The softer option keeps the sweep running, and it still refuses to call the result proof. It uses machinery this project built for that case, and it adds no default value.

The precedent for a refusal is close but not exact. `fiddle-t36b` refused to attempt a repair on an unreadable count. `fiddle-je9h` exited 11 rather than report a clean sweep from a refused read. Both refused when the thing they needed was unreadable. Here the findings are readable, and only the metadata is absent.

The project chose the refusal for one reason. A hard stop is easier to notice than a soft downgrade. A run that stops gets read. A run that files a provisional verdict every night can continue for weeks.
