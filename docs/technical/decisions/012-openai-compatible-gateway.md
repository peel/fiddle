# 012 — M1 talks to an OpenAI-compatible gateway, not to Anthropic

Status: accepted; amended in M4b by 050, which replaces "Suppressing the body was right and stays"
Cites: crates/fiddle-runtime/src/gateway.rs, GatewayModel, completion_model, completions_api, AgentError, agent::classify, agent::provider_fault, OutputMode::Tool, crates/fiddle-cli/tests/smoke.rs, scripts/tier2.sh, crates/fiddle-acceptance/tests/binary_repair.rs

## Context

The RFC specifies Rig's Anthropic integration, names `model = "claude-sonnet-4-5"`, and lists an Anthropic API key as the model-provider credential. No Anthropic key exists for this project. What exists is a LiteLLM virtual key fronting an OpenAI-compatible gateway with a $100 hard cap.

## Decision

Construct Rig's OpenAI client against that gateway. Build the only credential-carrying model in `crates/fiddle-runtime/src/gateway.rs`, and name the provider integration once as the `GatewayModel` alias. Select the chat-completions route with `.completions_api()`, because that is LiteLLM's most exercised translation path.

## Consequences

- A translating gateway makes a real-model lane load-bearing. The deterministic suite cannot see a translation defect, because `MockCompletionModel` replaces the provider and serialises no request.
- `binary_repair.rs` serialises a real chat-completions request and answers it from its own loopback socket. It proves this client speaks the wire format, never that the gateway translates it faithfully.
- The project gave up the scheduled canary its plan promised, and got two opt-in local lanes instead. Nothing runs unattended, so a gateway-side regression waits for a human.
- Budget is a failure class this build cannot name. An exhausted spend cap reads as a broken capability, and the next reader debugs the wrong thing.
- Model names come from the gateway, not from the RFC. `claude-sonnet-4-5` is not on this gateway.

## The credential and the configuration

The RFC's intent survives where it matters. The runtime constructs one authenticated provider, no capability and no tool sees the key, and `api_key` deserializes only from `{ env = "NAME" }`. A document carrying a literal value fails to load.

## The real-model proof is two opt-in local lanes

The plan's answer to "what proves the translation?" was a scheduled canary that emitted a degraded payload when no credential was present. **It was not built.** No workflow carries a `schedule:` trigger, no crate carries a canary subcommand, and no code carries a degraded payload. This ADR records the substitution, because the plan is a local artifact and a reader who looks for the canary will find neither it nor the text that promised it.

Two lanes exist instead, both opt-in and local, and neither invoked from a workflow.

- **Tier 1** is one `#[ignore]`d test in `crates/fiddle-cli/tests/smoke.rs`, driving the compiled binary against the gateway over a trivial fixture. It asserts protocol: the run reached the capability asked for, concluded on a row of the exit-code table, published a bundle that parses, wrote the marker exactly when the check earned it, and leaked the credential nowhere. It never asserts that the model succeeded.
- **Tier 2** is `scripts/tier2.sh`, run on demand over three harder fixtures. It writes one JSON record per fixture plus a `summary.json`, and exits 0 whatever the model made of them.

The schedule was dropped rather than deferred because `gh secret list --repo peel/fiddle --json name` returns `[]`. Provisioning a repository secret is a human act outside this milestone, so every firing would have emitted the degraded payload and nothing else. A scheduled lane can be built on Tier 2 once the secret exists.

Tier 1 panics when the credential is absent, naming the variable and how to load it. The plan weighed failing loudly, passing silently and skipping visibly, and chose the third. Tier 1 takes the first, for a reason the plan was not weighing: `#[ignore]` already keeps the lane off every gate, so only somebody who asked for the lane by name can reach the panic. The cost is that a Tier 1 run that never happened leaves no artifact, so "nobody has checked the translation in a month" looks exactly like "somebody checked it and it was fine".

## Structured output and tool use interact badly

Measured at the gateway. `tools` alone answers `finish_reason: tool_calls` with a call. `tools` plus `response_format: {type: json_schema}` answers `finish_reason: stop` and `tool_calls: null`, with the report filled in from nothing. That measurement stands.

The mechanism this ADR first gave for the fix does not. It said `OutputMode::Auto` resolves per provider through `composes_native_output_with_tools()`, and that stating `OutputMode::Tool` explicitly therefore fixed it. No such function exists in this build.

**`OutputMode::Tool` is inert on this path.** `rig_agent`'s `TypedPromptRequest::from_agent` overwrites the agent's output mode with `Native` unconditionally, so the builder line in `agent::attempt` changes nothing. `the_serialized_request_offers_six_tools_and_carries_no_host_fact` reads the bytes the compiled binary puts on its socket, and pins five tools on every turn, no synthetic output tool on any turn, and the native `response_format` constraint on the finalising turn alone. Deleting the line produces byte-identical traffic. A malformed report is still `AgentError::Protocol`, by Native's path rather than by a Tool-mode fallback.

**What made the tool loop start working is not established.** Commit `e993f4a` added the inert line and changed the default model from `claude-haiku-4-5` to `bedrock/moonshotai.kimi-k2.5` together. The line is inert, which leaves the model change as the only candidate, and the table below is consistent with it. That is inference from a table measured after the change, not an isolated experiment. The line stands rather than being deleted, because removing it is a behaviour change that only a per-mode real-model measurement could justify. `docs/BACKLOG.md` records that cost under 2026-08-09.

## Budget is not distinguishable from a capability fault

Recorded as open rather than asserted as satisfied, because an accepted ADR claiming a requirement it does not meet is worse than one that admits the gap.

`AgentError` carries four variants, `Bounded`, `Cancelled`, `Protocol` and `Provider`, and `agent::classify` matches Rig's typed variants rather than its message text, deliberately. A spend-cap refusal is an HTTP error with no typed variant, so it falls to the wildcard arm as `Provider { reason }`. Commit `4b2333b` reduced `agent::provider_fault` to "the gateway answered <status>" for any error carrying a status, because an error body is where a credential echo surfaces. ADR 050 narrowed that in M4b: the reason now quotes the body with the resolved credential replaced exactly. A human reading a Tier 2 artifact now sees the sentence the gateway wrote, so a spend cap that names itself is readable. This project has never captured that response, so the sentence it carries is unknown. No code separates the two, because no variant and no field types the difference. Read the earlier claim, that the reason text told them apart in code, as withdrawn.

Suppressing the body was right for as long as fiddle could not redact it, and ADR 050 records what changed. What is still missing is a typed signal beside the status, which is the opposite of parsing prose. Closing the gap needs one observation first: a key minted with a token `max_budget`, spent, and the response captured. Then either a fifth `AgentError` variant, or a typed field on `Provider` that `tier2.sh` can key on. Writing a classifier against a guessed string would be worse than the gap, because it fails open while claiming the coverage and no test can pin it. `docs/BACKLOG.md` records it as deferred debt under 2026-08-09.

## Model names come from the gateway

Measured at reference-configuration bounds over the trivial Tier 1 fixture, once the tool loop worked.

| model | tool calls | outcome |
|---|---|---|
| `claude-haiku-4-5` | 1 | check failed |
| `claude-sonnet-5` | 1 | report failed its schema |
| `bedrock/moonshotai.kimi-k2.5` | 6–7 | completed |
| `deepseek.v3.2` | 7 | completed |
| `zai.glm-5` | 7 | completed |

Both lanes default to kimi. The plan reasoned that a Claude-family model would be the most exercised translation path on a Claude-centric gateway, and the measurement is the other way round. The deterministic suite could not have told us either way.

This supersedes no earlier ADR. It records a deviation from the RFC and one from M1's own plan, so both survive in a committed document. A later milestone that acquires an Anthropic key, or a repository secret for this gateway, can revisit either.
