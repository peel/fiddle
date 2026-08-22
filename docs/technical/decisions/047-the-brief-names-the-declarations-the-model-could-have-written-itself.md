# 047 — The brief names the declarations the model could have written itself

Status: accepted
Cites: fiddle_runtime::agent::briefed, NAMED_DECLARATIONS, HOW_TO_WRITE_A_DECLARATION, DECLARED_COMMANDS, fiddle_runtime::workspace::declared::nameable, the_model_could_write_it, appendable, spell, Undeclared, the_brief_names_the_program_the_deployment_declared, the_brief_withholds_a_declaration_that_carries_a_host_path, the_brief_names_no_ecosystem_that_the_deployment_did_not_declare, the_rule_the_brief_applies_is_the_rule_the_tool_applies, binary_repair::the_serialized_request_names_a_declared_program_and_no_declarations_host_path

## Context

Run 32581705211 declared two programs, registered `run_command`, and called it zero times. It rewrote `go.sum` with `write_file` instead, from 985 lines to 2, and `go build ./...` then failed on every dependency. ADR 044 made the refusal the only channel that names the declared set, so a model that never guesses never learns it.

## Decision

Name each declared program and the arguments it fixes in the brief, in the words `spell` gives the refusal. Name a declaration only where the model could have written every word of it: `nameable` applies `appendable` to the declaration's own arguments and requires a bare program name. Withhold every other declaration.

## Why a declaration can carry a host fact

ADR 044 leaves a declaration's own arguments unbounded, so `program = "/usr/local/bin/go"` is a valid declaration and is a host fact by ADR 034's definition. Two other answers cost more. A basename is a name `resolve` then refuses, because `resolve` compares the whole program string. A refusal at load removes the asymmetry ADR 044 states deliberately, and stops a deployment pointing a declared program outside the project.

## Consequences

- **The brief lists what the model can write, and does not claim to be the whole list.** ADR 044's sentence stays in it, so a withheld declaration is reachable through the refusal alone.
- **The rule the brief applies is the rule the tool applies to the model.** `nameable` calls `appendable`, the function that refuses the model an absolute path and a `..` segment.
- **fiddle still names no ecosystem.** A program name reaches the brief from the deployment's document alone, and `the_brief_names_no_ecosystem_that_the_deployment_did_not_declare` reads fiddle's own three constants.
- **The claim is asserted against the serialized outbound request.** One deployment declares a bare program, one absolute program and one absolute argument, and the bodies name the first alone.
- **What was given up: a refusal still reads every declaration back verbatim, including one the brief withheld.** Narrowing `Undeclared` changes what ADR 044 made the teaching channel, so BACKLOG carries it.
