# Provider/model benchmark history

Harness: forced_tool_call + prompt_tool_result_interaction:v1; clients: kilo, claude, pi, opencode.

Each run records deterministic tool calls plus prompt → tool-result interactions. Latency is local observed wall time; it includes provider/browser latency and is not a proxy-throughput claim.

| Generated | Providers worked/fetched | Models worked/fetched | Logs fetched | Requests | Passed | Failed | Total ms | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 2026-08-11T14:54:39.311Z | 1/3 | 8/236 | 24 | 72 | 40 | 32 | n/a | 1798.661 | 4404.25 |
| 2026-08-11T14:59:17.699Z | 1/3 | 8/236 | 24 | 72 | 40 | 32 | 175239.798 | 1605.397 | 4294.41 |
| 2026-08-11T17:28:58.035Z | 0/1 | 0/9 | 1 | 3 | 0 | 3 | 1745.383 | 466.454 | 1225.133 |
| 2026-08-11T17:33:13.059Z | 0/1 | 0/9 | 9 | 27 | 0 | 27 | 9398.479 | 452.091 | 555.919 |
| 2026-08-11T17:42:19.399Z | 0/1 | 0/9 | 9 | 27 | 0 | 27 | 8794.364 | 439.772 | 552.747 |
| 2026-08-11T17:45:04.392Z | 0/1 | 0/9 | 9 | 27 | 0 | 27 | 14785.497 | 421.397 | 805.077 |
| 2026-08-11T17:45:34.788Z | 0/1 | 0/9 | 9 | 27 | 0 | 27 | 27654.975 | 418.39 | 666.748 |

## Latest conversation results

| Kind | Provider | Model | Protocol | HTTP | Latency ms | Output preview | Result |
|---|---|---|---|---:|---:|---|---|
| tool_call | qwen | qwen:qwen3.7-max | openai-chat-completions | 502 | 428.675 | n/a | failed |
| tool_call | qwen | qwen:qwen3.7-max-no-thinking | openai-chat-completions | 502 | 472.837 | n/a | failed |
| tool_call | qwen | qwen:qwen3.7-max-thinking | openai-chat-completions | 502 | 422.117 | n/a | failed |
| tool_call | qwen | qwen:qwen3.7-plus | openai-chat-completions | 502 | 416.928 | n/a | failed |
| tool_call | qwen | qwen:qwen3.7-plus-no-thinking | openai-chat-completions | 502 | 416.497 | n/a | failed |
| tool_call | qwen | qwen:qwen3.7-plus-thinking | openai-chat-completions | 502 | 20099.285 | n/a | failed |
| tool_call | qwen | qwen:qwen3.8-max | openai-chat-completions | 502 | 427.732 | n/a | failed |
| tool_call | qwen | qwen:qwen3.8-max-no-thinking | openai-chat-completions | 502 | 426.777 | n/a | failed |
| tool_call | qwen | qwen:qwen3.8-max-thinking | openai-chat-completions | 502 | 666.748 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max | anthropic-messages | 404 | 1.368 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max-no-thinking | anthropic-messages | 404 | 1.082 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max-thinking | anthropic-messages | 404 | 1.208 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus | anthropic-messages | 404 | 1.038 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus-no-thinking | anthropic-messages | 404 | 1.053 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus-thinking | anthropic-messages | 404 | 1 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max | anthropic-messages | 404 | 0.96 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max-no-thinking | anthropic-messages | 404 | 1.071 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max-thinking | anthropic-messages | 404 | 0.991 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max | openai-chat-completions | 502 | 423.249 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max-no-thinking | openai-chat-completions | 502 | 421.273 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-max-thinking | openai-chat-completions | 502 | 413.01 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus | openai-chat-completions | 502 | 421.104 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus-no-thinking | openai-chat-completions | 502 | 421.033 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.7-plus-thinking | openai-chat-completions | 502 | 423.485 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max | openai-chat-completions | 502 | 411.732 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max-no-thinking | openai-chat-completions | 502 | 418.39 | n/a | failed |
| prompt_tool_result_interaction | qwen | qwen:qwen3.8-max-thinking | openai-chat-completions | 502 | 452.752 | n/a | failed |
