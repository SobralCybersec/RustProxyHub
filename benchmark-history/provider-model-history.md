# Provider/model benchmark history

Harness: forced_tool_call + prompt_tool_result_interaction:v1; clients: kilo, claude, pi, opencode.

Each run records deterministic tool calls plus prompt → tool-result interactions. Latency is local observed wall time; it includes provider/browser latency and is not a proxy-throughput claim.

| Generated | Providers worked/fetched | Models worked/fetched | Logs fetched | Requests | Passed | Failed | Total ms | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-08-11T14:54:39.311Z | 1/3 | 8/236 | 24 | 72 | 40 | 32 | n/a | 1798.661 | 4404.25 |
| 2026-08-11T14:59:17.699Z | 1/3 | 8/236 | 24 | 72 | 40 | 32 | 175239.798 | 1605.397 | 4294.41 |

## Latest conversation results

| Kind | Provider | Model | Protocol | HTTP | Latency ms | Output preview | Result |
|---|---|---|---|---:|---:|---|---|
| tool_call | chatgpt | chatgpt:chatGPT | openai-chat-completions | 200 | 2940.528 | n/a | passed |
| tool_call | chatgpt | chatgpt:ChatGPT | openai-chat-completions | 200 | 2689.538 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_android_dictation_while_typing_enabled | openai-chat-completions | 200 | 2829.397 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_android_home_page_starter_prompts_anon_v2 | openai-chat-completions | 200 | 5323.382 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_android_native_streaming_dictation_enabled | openai-chat-completions | 200 | 1811.127 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_android_starter_prompts_use_case | openai-chat-completions | 200 | 2701.457 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_checkout_billing_address_prefill_20260616 | openai-chat-completions | 200 | 2498.634 | n/a | passed |
| tool_call | chatgpt | chatgpt:chatgpt_conversation_reporting_disabled | openai-chat-completions | 200 | 2153.317 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-expert | openai-chat-completions | 200 | 3514.465 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-expert-deepthink | openai-chat-completions | 200 | 4294.41 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-instant | openai-chat-completions | 200 | 2917.975 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-instant-deepthink | openai-chat-completions | 200 | 2985.827 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-v4-flash | openai-chat-completions | 200 | 2443.25 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-v4-flash-thinking | openai-chat-completions | 200 | 3423.732 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-v4-pro | openai-chat-completions | 200 | 4097.33 | n/a | passed |
| tool_call | deepseek | deepseek:deepseek-v4-pro-thinking | openai-chat-completions | 200 | 3793.478 | n/a | passed |
| tool_call | qwen | qwen:qwen-latest-series-invite-beta-v16 | openai-chat-completions | 200 | 1647.577 | n/a | failed |
| tool_call | qwen | qwen:qwen-latest-series-invite-beta-v16-no-thinking | openai-chat-completions | 200 | 1098.743 | n/a | failed |
| tool_call | qwen | qwen:qwen-latest-series-invite-beta-v24 | openai-chat-completions | 200 | 935.343 | n/a | failed |
| tool_call | qwen | qwen:qwen-latest-series-invite-beta-v24-no-thinking | openai-chat-completions | 200 | 873.34 | n/a | failed |
| tool_call | qwen | qwen:qwen-plus-2025-07-28 | openai-chat-completions | 200 | 892.772 | n/a | failed |
| tool_call | qwen | qwen:qwen-plus-2025-07-28-no-thinking | openai-chat-completions | 200 | 910.996 | n/a | failed |
| tool_call | qwen | qwen:qwen3-coder-plus | openai-chat-completions | 200 | 924.943 | n/a | failed |
| tool_call | qwen | qwen:qwen3-coder-plus-no-thinking | openai-chat-completions | 200 | 941.426 | n/a | failed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatGPT | anthropic-messages | 200 | 3323.657 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:ChatGPT | anthropic-messages | 200 | 2786.783 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_dictation_while_typing_enabled | anthropic-messages | 200 | 8656.305 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_home_page_starter_prompts_anon_v2 | anthropic-messages | 200 | 2171.51 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_native_streaming_dictation_enabled | anthropic-messages | 200 | 1473.573 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_starter_prompts_use_case | anthropic-messages | 200 | 2180.887 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_checkout_billing_address_prefill_20260616 | anthropic-messages | 200 | 2273.13 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_conversation_reporting_disabled | anthropic-messages | 200 | 2204.015 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-expert | anthropic-messages | 404 | 1.22 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-expert-deepthink | anthropic-messages | 404 | 1.062 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-instant | anthropic-messages | 404 | 1.083 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-instant-deepthink | anthropic-messages | 404 | 1.011 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-flash | anthropic-messages | 404 | 0.908 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-flash-thinking | anthropic-messages | 404 | 0.927 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-pro | anthropic-messages | 404 | 0.888 | n/a | failed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-pro-thinking | anthropic-messages | 404 | 1.045 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v16 | anthropic-messages | 404 | 0.866 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v16-no-thinking | anthropic-messages | 404 | 0.891 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v24 | anthropic-messages | 404 | 1.207 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v24-no-thinking | anthropic-messages | 404 | 0.963 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-plus-2025-07-28 | anthropic-messages | 404 | 0.953 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-plus-2025-07-28-no-thinking | anthropic-messages | 404 | 0.9 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3-coder-plus | anthropic-messages | 404 | 0.951 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3-coder-plus-no-thinking | anthropic-messages | 404 | 0.885 | n/a | failed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatGPT | openai-chat-completions | 200 | 1682.13 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:ChatGPT | openai-chat-completions | 200 | 4202.861 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_dictation_while_typing_enabled | openai-chat-completions | 200 | 2057.237 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_home_page_starter_prompts_anon_v2 | openai-chat-completions | 200 | 1970.154 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_native_streaming_dictation_enabled | openai-chat-completions | 200 | 1605.397 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_android_starter_prompts_use_case | openai-chat-completions | 200 | 2968.175 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_checkout_billing_address_prefill_20260616 | openai-chat-completions | 200 | 1260.09 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | chatgpt | chatgpt:chatgpt_conversation_reporting_disabled | openai-chat-completions | 200 | 1543.949 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-expert | openai-chat-completions | 200 | 1406.552 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-expert-deepthink | openai-chat-completions | 200 | 19526.055 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-instant | openai-chat-completions | 200 | 2670.371 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-instant-deepthink | openai-chat-completions | 200 | 2961.574 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-flash | openai-chat-completions | 200 | 2271.443 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-flash-thinking | openai-chat-completions | 200 | 2752.474 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-pro | openai-chat-completions | 200 | 2163.252 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | deepseek | deepseek:deepseek-v4-pro-thinking | openai-chat-completions | 200 | 2709.203 | RUST_PROXY_HUB_INTERACTION_CONFIRMED | passed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v16 | openai-chat-completions | 200 | 891.226 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v16-no-thinking | openai-chat-completions | 200 | 886.907 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v24 | openai-chat-completions | 200 | 889.553 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-latest-series-invite-beta-v24-no-thinking | openai-chat-completions | 200 | 934.136 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-plus-2025-07-28 | openai-chat-completions | 200 | 918.376 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen-plus-2025-07-28-no-thinking | openai-chat-completions | 200 | 1331.1 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3-coder-plus | openai-chat-completions | 200 | 879.893 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3-coder-plus-no-thinking | openai-chat-completions | 200 | 919.751 | n/a | failed |
