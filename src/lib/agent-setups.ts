import type { ProviderName, ProviderOverview } from '@/lib/types'

export type AgentSetupKind = 'pi' | 'claude' | 'kilo'

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
  zai: ['glm-5.2', 'glm-5.1', 'glm-5-turbo'],
}

function providerModels(provider: ProviderOverview) {
  return provider.models.length ? provider.models : fallbackModels[provider.name]
}

function primaryModel(provider: ProviderOverview) {
  return providerModels(provider)[0]
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
            apiKey: '<set-your-RUST_PROXY_HUB_API_KEY>',
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
  const model = primaryModel(provider)
  return {
    supported: true,
    target: '~/.claude/settings.json',
    summary: `Claude Code settings snippet for ${provider.name} -> ${provider.base_url}`,
    content: JSON.stringify(
      {
        env: {
          ANTHROPIC_BASE_URL: provider.base_url,
          ANTHROPIC_AUTH_TOKEN: '<set-your-RUST_PROXY_HUB_API_KEY>',
          ANTHROPIC_API_KEY: '<set-your-RUST_PROXY_HUB_API_KEY>',
          ANTHROPIC_DEFAULT_OPUS_MODEL: model,
          ANTHROPIC_DEFAULT_SONNET_MODEL: model,
          ANTHROPIC_DEFAULT_HAIKU_MODEL: model,
          CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS: '1',
        },
      },
      null,
      2,
    ),
  }
}

export function buildKiloSetup(provider: ProviderOverview): AgentSetup {
  const model = primaryModel(provider)
  return {
    supported: true,
    target: 'kilo.json',
    summary: `Kilo openai-compatible snippet for ${provider.name} -> ${provider.base_url}`,
    content: JSON.stringify(
      {
        model: `openai-compatible/${model}`,
        provider: {
          'openai-compatible': {
            options: {
              apiKey: '<set-your-RUST_PROXY_HUB_API_KEY>',
              baseURL: `${provider.base_url}/v1`,
            },
            models: {
              [model]: {
                name: `${provider.name} ${model}`,
                tool_call: true,
              },
            },
          },
        },
      },
      null,
      2,
    ),
  }
}
