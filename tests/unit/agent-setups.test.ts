import { describe, expect, it } from 'vitest'
import { buildClaudeSetup, buildPiSetup } from '@/lib/agent-setups'
import type { ProviderOverview } from '@/lib/types'

function makeProvider(overrides: Partial<ProviderOverview> = {}): ProviderOverview {
  return {
    name: 'deepseek',
    running: true,
    started_at: 1,
    base_url: 'http://127.0.0.1:3001',
    health_status: 'ok',
    login_state: 'ready',
    model_count: 2,
    models: ['deepseek-v4-pro', 'deepseek-v4-pro-thinking'],
    web_search_supported: true,
    last_error: null,
    ...overrides,
  }
}

describe('agent setup helpers', () => {
  it('builds a Pi models.json snippet from provider endpoint and models', () => {
    const setup = buildPiSetup(makeProvider())

    expect(setup.supported).toBe(true)
    expect(setup.target).toBe('~/.pi/agent/models.json')
    expect(setup.content).toContain('"baseUrl": "http://127.0.0.1:3001/v1"')
    expect(setup.content).toContain('"id": "deepseek-v4-pro-thinking"')
  })

  it('falls back to known model ids when provider discovery is empty', () => {
    const setup = buildPiSetup(makeProvider({ name: 'chatgpt', base_url: 'http://127.0.0.1:3003', models: [] }))

    expect(setup.content).toContain('"id": "chatgpt-web-session"')
  })

  it('builds an honest Claude note for OpenAI-compatible localhost providers', () => {
    const setup = buildClaudeSetup(makeProvider({ name: 'kimi', base_url: 'http://127.0.0.1:3002' }))

    expect(setup.supported).toBe(false)
    expect(setup.target).toBe('~/.claude/settings.json')
    expect(setup.content).toContain('Claude Code cannot use this endpoint directly.')
    expect(setup.content).toContain('Anthropic Messages (/v1/messages)')
    expect(setup.content).toContain('http://127.0.0.1:3002/v1')
  })
})
