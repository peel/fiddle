# 012 — M1 talks to an OpenAI-compatible gateway, not to Anthropic

Status: accepted

## Context

The RFC specifies Rig's Anthropic integration, configured with an Anthropic API key: its *Agent execution implementations* section names `model = "claude-sonnet-4-5"` with `api_key = { env = "ANTHROPIC_API_KEY" }`, and its *Credentials and trust boundaries* section lists "an Anthropic API key resolved by the host and used to construct Rig's Anthropic client" as the model-provider credential.

No Anthropic key exists for this project. What exists is a LiteLLM virtual key, `LITELLM_API_KEY`, fronting an OpenAI-compatible gateway at `https://litellm.firn.snplow.net/v1`, with a **$100 hard cap**. Building M1 against a credential nobody has would have made the milestone unverifiable against a real model, which is the one thing M1 is for.

## Decision

M1 constructs Rig's OpenAI client against that gateway. `crates/fiddle-runtime/src/gateway.rs` is the only place a credential-carrying model is built, and `GatewayModel` is one type alias, so "which provider integration is in use" is a single line a future change has to edit deliberately.

The gateway routes both `/v1/chat/completions` and `/v1/responses`. `.completions_api()` selects the former, because chat-completions is LiteLLM's most exercised translation path from an OpenAI-shaped request to a non-OpenAI upstream — which is exactly what this deployment is doing.

The RFC's intent is preserved where it matters. One authenticated provider is constructed by the runtime; no capability and no tool ever sees the key; and configuration names a variable and cannot hold a value — `api_key` deserializes only from `{ env = "NAME" }`, so a document carrying a literal fails to load at all.

## Consequences

**A translating gateway makes the live canary load-bearing.** Tool calls go out in OpenAI function-calling shape and are translated to the upstream provider. The deterministic suite structurally cannot see a translation defect, because `MockCompletionModel` replaces the provider entirely and never serialises a request to anyone. This is not a theoretical concern: it is exactly how the `OutputMode` defect below was found, and exactly how it would otherwise have shipped green.

**`OutputMode::Auto` is wrong for a compatible endpoint.** Rig's `Auto` resolves per provider, keeping native structured output whenever the provider reports `composes_native_output_with_tools()`. `openai::completion::CompletionModel` reports `true` — a true statement about OpenAI's own endpoint, and a false one about an OpenAI-compatible endpoint fronting Anthropic. So `Auto` pinned `response_format` and the model called no tools at all: not rarely, never. Measured directly at the gateway, `tools` alone comes back `finish_reason: tool_calls` with a call; `tools` plus `response_format: {type: json_schema}` comes back `finish_reason: stop`, `tool_calls: null`, with the report filled in from nothing. `OutputMode::Tool` is therefore stated explicitly in `agent::attempt` rather than inferred. The cost is stated honestly: Tool mode is best-effort where Native was guaranteed, so the report is validated afterwards and a malformed one is `AgentError::Protocol`. A guarantee bought by never letting the model use a tool is worthless.

**Budget is a failure class.** A $100 hard cap means requests begin failing on *spend*, not on correctness. `budget_exhausted` has to stay distinguishable from `auth`, `quota`, `timeout`, `provider` and `capability` in whatever the canary reports, or an exhausted budget reads as a broken capability and the next reader debugs the wrong thing.

**Model names come from the gateway, not from the RFC.** `claude-sonnet-4-5` is not on this gateway. Measured at reference-configuration bounds over the trivial Tier 1 fixture, once the tool loop worked at all:

| model | tool calls | outcome |
|---|---|---|
| `claude-haiku-4-5` | 1 | check failed |
| `claude-sonnet-5` | 1 | report failed its schema |
| `bedrock/moonshotai.kimi-k2.5` | 6–7 | **completed** |
| `deepseek.v3.2` | 7 | **completed** |
| `zai.glm-5` | 7 | **completed** |

Tier 1 and Tier 2 therefore default to kimi. The plan's rationale — that a Claude-family model would be the most exercised translation path on a Claude-centric gateway — was empirically backwards, and the deterministic suite could not have told us either way.

This supersedes nothing. It records a deviation the RFC did not anticipate, and a later milestone that acquires an Anthropic key can revisit it knowing what the gateway cost.
