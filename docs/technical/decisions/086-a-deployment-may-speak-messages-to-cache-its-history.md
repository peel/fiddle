# 086 — A deployment may speak Messages to its gateway, and then its history is cached

Status: accepted; amends 012
Cites: crates/fiddle-runtime/src/gateway.rs, Protocol, GatewayModel, GatewayResponse, completion_model, with_prompt_caching, composes_native_output_with_tools, STREAMING_UNSUPPORTED, crates/fiddle-runtime/src/agent/spend.rs, SpendHook, crates/fiddle-runtime/src/agent/transcript.rs, crates/fiddle-runtime/tests/gateway_messages.rs, a_messages_request_marks_the_system_prompt_the_last_tool_and_the_last_message, a_chat_completions_request_carries_no_breakpoint, a_real_gateway_reads_back_the_prefix_the_turn_before_it_cached, both_protocols_keep_the_providers_own_answer_on_output_and_tools, crates/fiddle-cli/tests/smoke.rs, FIDDLE_TIER1_PROTOCOL

## Context

Every turn of an agent run resends the whole history. On 2026-09-23 a live toil run made 129 calls and sent 7,020,009 input tokens. Almost all of that was the history the run had already sent.

Anthropic's prompt cache bills a read of a cached prefix at a tenth of base input. It needs a `cache_control` breakpoint on the request. Fiddle set none.

ADR 012 chose the chat-completions route of an OpenAI-compatible gateway. This record measures that route and the gateway's native Messages route for a cache.

This record grades each claim as ADR 083 does. MEASURED means a test in this repository observes it, or it was read off a real response. ARGUED means it was read off the source and no test fails if it is wrong.

## What the gateway does with a breakpoint

MEASURED on 2026-09-24 and 2026-09-25 against `https://litellm.firn.snplow.net`, `claude-sonnet-5`, one 6220-character prefix with a breakpoint:

| route | call 1 | call 2 | call 3 |
|---|---|---|---|
| `/v1/chat/completions` | written 2012, read 0 | written 2012, read 0 | written 2012, read 0 |
| `/v1/messages` | written 2012, read 0 | written 0, read 2012 | written 0, read 2012 |

On chat-completions the cache is written on every call and read on none. That route costs more with a breakpoint than without one, because a write bills at 1.25 times base input. On Messages the second call reads what the first wrote.

## Decision

**`[agent] protocol` chooses the route. `chat-completions` is the default and is unchanged. `messages` speaks the Anthropic Messages protocol and sets breakpoints.**

- `GatewayModel` is an enum over rig's OpenAI chat-completions model and rig's Anthropic model. `completion_model` builds one of the two from one read of the credential, as before.
- On `messages` the model is built `with_prompt_caching`. rig then puts a breakpoint on the system prompt, the last tool definition and the last message.
- The wrapper forwards `composes_native_output_with_tools` to the model it holds. rig's default is `false`, and both providers answer `true`. A wrapper that did not forward it would change how the repair step's typed prompt is sent on both routes. `both_protocols_keep_the_providers_own_answer_on_output_and_tools` holds that.
- `stream` refuses with `STREAMING_UNSUPPORTED`. This build never streams a completion, and a refusal is better than a mapped stream that could lose data.
- `config check` names the protocol and whether prompt caching is on, in both renderings.

The default stays `chat-completions` because the same gateway serves models that are not Anthropic's over that route. ADR 012's table records `bedrock/moonshotai.kimi-k2.5`, `deepseek.v3.2` and `zai.glm-5`. This record does not measure those models on `messages`.

## The spend bound counts what was cached

MEASURED by `a_turn_read_from_the_cache_still_counts_toward_the_bound`. On Messages, `input_tokens` does not include the tokens read from or written to the cache. `SpendHook` counted input plus output. With the history cached, that sum stays small, and a run that does not converge would pass the bound. The hook now counts `total_tokens`, which includes the cache on both routes. It falls back to input plus output when a provider reports no total.

## The proof against a real gateway

MEASURED on 2026-09-25 by the Tier 1 lane in `crates/fiddle-cli/tests/smoke.rs`, which now takes `FIDDLE_TIER1_PROTOCOL` and reports the transcript's token counts. Same model, same fixture, one run each:

| route | outcome | turns | fresh input | read from cache | written to cache | output |
|---|---|---|---|---|---|---|
| `chat-completions` | completed, repair landed | 6 | 21,526 | 0 | 0 | 896 |
| `messages` | completed, repair landed | 5 | 10 | 19,780 | 1,917 | 480 |

In base-input units the chat-completions run cost 21,526. The Messages run cost 10 + 1,978 + 2,396 = 4,384. That is about a fifth. The history grows with every turn, so the fraction falls as a run gets longer. One run per route is a small sample, and the lane asserts protocol, not cost.

`a_real_gateway_reads_back_the_prefix_the_turn_before_it_cached` is an `#[ignore]`d row that drives `GatewayModel` itself against a real gateway. It asserts that the second of two turns reads back what the first wrote.

## Native structured output on Messages

MEASURED once, by hand, on 2026-09-25. The repair step asks the provider for structured output. On Messages, rig sends that as `output_config.format`. The gateway forwarded it, with a tool also offered, and the model answered one JSON object with no envelope and no fence. ADR 085 records five different wrappings of the same request on chat-completions. One answer does not establish that Messages avoids them. It is recorded so that the next measurement has a starting point.

## Consequences

- A deployment gets the cache only when it sets `protocol = "messages"`. No deployment in this repository sets it yet.
- A deployment on `messages` must use a gateway, and a model, that speaks the Anthropic Messages protocol. rig removes a trailing `/v1` from `base_url`, so a URL written for chat-completions still reaches `/v1/messages`.
- The cache lives for five minutes. A run that pauses longer, for example while a tool runs, writes the prefix again.
- The request-shape rows in `crates/fiddle-acceptance` still assert chat-completions bytes. They describe the default route and stay correct. No acceptance row runs a whole capability on `messages`. The loopback rows in `crates/fiddle-runtime/tests/gateway_messages.rs` hold the request, and the Tier 1 lane holds one real run.
