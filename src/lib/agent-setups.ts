import type { ProviderName, ProviderOverview } from '@/lib/types'

export type AgentSetupKind = 'pi' | 'claude'

export interface AgentSetup {
  supported: boolean
  target: string
  content: string
  summary: string
}

const fallbackModels: Record<ProviderName, string[]> = {
  qwen: ['qwen-plus-2025-07-28'],
  deepseek: ['deepseek-v4-pro', 'deepseek-v4-pro-thinking'],
  kimi: ['kimi-k2.6', 'kimi-k2.6-thinking'],
  chatgpt: ['chatgpt-web-session'],
  gemini: ['gemini-web-session'],
  mistral: ['mistral-web-session'],
}

function providerModels(provider: ProviderOverview) {
  return provider.models.length ? provider.models : fallbackModels[provider.name]
}

export function buildPiSetup(provider: ProviderOverview): AgentSetup {
  const models = providerModels(provider).map((id) => ({ id }))
  return {
    supported: true,
    target: '~/.pi/agent/models.json',
    summary: `Pi models.json snippet for ${provider.name} -> ${provider.base_url}`,
    content: JSON.stringify(
      {
        providers: {
          [`rustproxyhub-${provider.name}`]: {
            baseUrl: `${provider.base_url}/v1`,
            api: 'openai-completions',
            apiKey: 'local',
            models,
          },
        },
      },
      null,
      2,
    ),
  }
}

export function buildClaudeSetup(provider: ProviderOverview): AgentSetup {
  return {
    supported: false,
    target: '~/.claude/settings.json',
    summary: `Claude Code note for ${provider.name} -> ${provider.base_url}`,
    content: [
      `RustProxyHub provider: ${provider.name}`,
      `Local endpoint: ${provider.base_url}/v1`,
      '',
      'Claude Code cannot use this endpoint directly.',
      'Reason: Claude Code gateway support requires Anthropic Messages (/v1/messages), Bedrock, or Vertex formats.',
      'RustProxyHub currently exposes OpenAI-compatible chat completions (/v1/chat/completions).',
      '',
      'Working options:',
      '1. Use Pi with this provider endpoint directly.',
      '2. Put an Anthropic-format gateway in front of a Claude-capable backend, then set:',
      '',
      JSON.stringify(
        {
          env: {
            ANTHROPIC_BASE_URL: 'https://your-anthropic-gateway.example.com',
            CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY: '1',
          },
        },
        null,
        2,
      ),
      '',
      'Docs:',
      'https://code.claude.com/docs/en/llm-gateway',
      'https://code.claude.com/docs/en/env-vars',
      'https://code.claude.com/docs/en/mcp',
    ].join('\n'),
  }
}
