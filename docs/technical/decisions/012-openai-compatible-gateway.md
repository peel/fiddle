# 012 — M1 talks to an OpenAI-compatible gateway, not to Anthropic

Status: accepted

## Context

The RFC specifies Rig's Anthropic integration, configured with an Anthropic API key: its *Agent execution implementations* section names `model = "claude-sonnet-4-5"` with `api_key = { env = "ANTHROPIC_API_KEY" }`, and its *Credentials and trust boundaries* section lists "an Anthropic API key resolved by the host and used to construct Rig's Anthropic client" as the model-provider credential.

No Anthropic key exists for this project. What exists is a LiteLLM virtual key, `LITELLM_API_KEY`, fronting an OpenAI-compatible gateway at `https://litellm.firn.snplow.net/v1`, with a **$100 hard cap**. Building M1 against a credential nobody has would have made the milestone unverifiable against a real model, which is the one thing M1 is for.

## Decision

M1 constructs Rig's OpenAI client against that gateway. `crates/fiddle-runtime/src/gateway.rs` is the only place a credential-carrying model is built, and `GatewayModel` is one type alias, so "which provider integration is in use" is a single line a future change has to edit deliberately.

The gateway routes both `/v1/chat/completions` and `/v1/responses`. `.completions_api()` selects the former, because chat-completions is LiteLLM's most exercised translation path from an OpenAI-shaped request to a non-OpenAI upstream — which is exactly what this deployment is doing.

The RFC's intent is preserved where it matters. One authenticated provider is constructed by the runtime; no capability and no tool ever sees the key; and configuration names a variable and cannot hold a value — `api_key` deserializes only from `{ env = "NAME" }`, so a document carrying a literal fails to load at all.

### The real-model proof is two opt-in local lanes, not a scheduled canary

The plan's answer to "what proves the translation?" was a scheduled, non-gating canary that, with no credential present, emitted `{"status":"degraded","reason":"missing_…","canary_exercised":false}` rather than passing silently. **It was not built.** There is no `schedule:` trigger in any workflow, no canary subcommand in any crate, and no degraded-status payload anywhere. This ADR records the substitution as a decision, because the plan that described the canary is a local lifecycle artifact and a reader who goes looking for the artifact will not find either it or the text that promised it.

What exists instead is two lanes, both opt-in, both local, neither scheduled and neither invoked from `.github/workflows`:

- **Tier 1** — `crates/fiddle-cli/tests/smoke.rs`: one `#[ignore]`d test driving the compiled binary against the gateway over a deliberately trivial fixture. It asserts protocol — the run reached the capability it was asked for, concluded on a row of the exit-code table, published a bundle that parses, wrote the correlation marker exactly when the check earned it, and leaked the credential nowhere. It never asserts that the model succeeded.
- **Tier 2** — `scripts/tier2.sh`: on demand, over three harder fixtures, writing one JSON record per fixture plus a `summary.json`, and exiting 0 whatever the model made of them.

The reason a schedule was dropped rather than deferred: `gh secret list --repo peel/fiddle --json name` returns `[]`, and provisioning a repository secret is a human act outside this milestone. A scheduled workflow would therefore have had nothing to run — every firing would have emitted the degraded payload and nothing else, a job whose only observable behaviour is announcing that it cannot do its job. Once the secret exists, a scheduled lane can be built on top of Tier 2, which already emits the machine-readable record such a lane would publish.

### What the substitution gives up

The plan weighed three behaviours for the credential-absent case — **fail loudly**, **pass silently**, and **skip visibly** — and chose the third precisely because it neither fails the build nor passes silently. **Tier 1 takes the first: it panics**, naming the variable and how to load it. That is the option the plan ruled out, taken here for a reason the plan was not weighing: `#[ignore]` already keeps the lane off every gate, so the panic is reachable only by someone who asked for the lane by name and did not load `.env`. For that reader a loud failure is right, and a silent skip would be M0's stale-binary defect wearing another hat. Nothing a gate runs can reach it, so the property the plan was protecting — the build does not fail for want of a credential — is bought by the lane not running at all rather than by a visible skip.

The cost is real, and the gate does not pay it. **There is no machine-readable record that the real-model path was not exercised.** A Tier 1 run that never happened leaves no artifact at all, so "nobody has checked the translation in a month" is indistinguishable from "somebody checked it and it was fine" — which is precisely the distinction the degraded payload existed to make. And nothing runs unattended: a translation regression on the gateway's side is caught when a human runs Tier 1 or Tier 2, and not before.

## Consequences

**A translating gateway makes a real-model lane load-bearing.** Tool calls go out in OpenAI function-calling shape and are translated to the upstream provider. The deterministic suite structurally cannot see a translation defect, because `MockCompletionModel` replaces the provider entirely and never serialises a request to anyone. Nor can `crates/fiddle-acceptance/tests/binary_repair.rs`, which does serialise real chat-completions requests but answers them from its own loopback socket: it proves our client speaks the wire format, never that the gateway's translation of it is faithful. Only Tier 1 and Tier 2 reach the gateway. This is not a theoretical concern: it is exactly how the `OutputMode` defect below was found, and exactly how it would otherwise have shipped green.

**`OutputMode::Auto` is wrong for a compatible endpoint.** Rig's `Auto` resolves per provider, keeping native structured output whenever the provider reports `composes_native_output_with_tools()`. `openai::completion::CompletionModel` reports `true` — a true statement about OpenAI's own endpoint, and a false one about an OpenAI-compatible endpoint fronting Anthropic. So `Auto` pinned `response_format` and the model called no tools at all: not rarely, never. Measured directly at the gateway, `tools` alone comes back `finish_reason: tool_calls` with a call; `tools` plus `response_format: {type: json_schema}` comes back `finish_reason: stop`, `tool_calls: null`, with the report filled in from nothing. `OutputMode::Tool` is therefore stated explicitly in `agent::attempt` rather than inferred. The cost is stated honestly: Tool mode is best-effort where Native was guaranteed, so the report is validated afterwards and a malformed one is `AgentError::Protocol`. A guarantee bought by never letting the model use a tool is worthless.

**Budget is a failure class, and separating it is an open consequence.** A $100 hard cap means requests begin failing on *spend*, not on correctness. `budget_exhausted` needs to stay distinguishable from `auth`, `quota`, `timeout`, `provider` and `capability`, or an exhausted budget reads as a broken capability and the next reader debugs the wrong thing.

**It is not distinguishable today.** Recorded here as open rather than asserted as satisfied policy, because an accepted ADR claiming a requirement it does not meet is worse than one that admits the gap. `AgentError` has four variants — `Bounded`, `Cancelled`, `Protocol`, `Provider` — and `agent::classify` matches Rig's *typed* variants rather than its message text, deliberately. A spend-cap refusal from the gateway is an HTTP error with no typed variant of its own, so it falls to the wildcard arm as `Provider { reason }` carrying nothing but Rig's rendering of the response body. `scripts/tier2.sh` records that outcome kind and the first 300 characters of the reason, so a *human* reading a Tier 2 artifact can tell an exhausted budget from a broken capability. Nothing machine-readable can, and Tier 1 does not classify at all.

Classifying it now would mean matching on the gateway's error *text* — the one thing `classify` avoids on purpose — and on text this project has never observed, because the cap has never been reached. A classifier written against a guessed string is worse than the acknowledged gap: it fails open silently (a spend-cap refusal still lands in `Provider`) while claiming the coverage, and no test can pin it. Closing it needs one observation first: a gateway key minted with a token `max_budget`, spent, and the response captured. Then either a fifth `AgentError` variant or a typed field on `Provider` that `tier2.sh` can key on without parsing prose. Recorded as deferred debt in `docs/BACKLOG.md` (2026-08-09).

**Model names come from the gateway, not from the RFC.** `claude-sonnet-4-5` is not on this gateway. Measured at reference-configuration bounds over the trivial Tier 1 fixture, once the tool loop worked at all:

| model | tool calls | outcome |
|---|---|---|
| `claude-haiku-4-5` | 1 | check failed |
| `claude-sonnet-5` | 1 | report failed its schema |
| `bedrock/moonshotai.kimi-k2.5` | 6–7 | **completed** |
| `deepseek.v3.2` | 7 | **completed** |
| `zai.glm-5` | 7 | **completed** |

Tier 1 and Tier 2 therefore default to kimi. The plan's rationale — that a Claude-family model would be the most exercised translation path on a Claude-centric gateway — was empirically backwards, and the deterministic suite could not have told us either way.

This supersedes no earlier ADR. It records a deviation the RFC did not anticipate, and one from M1's own plan — the scheduled canary above — so that both survive in a committed document. A later milestone that acquires an Anthropic key, or a repository secret for this gateway, can revisit either knowing what it cost.
