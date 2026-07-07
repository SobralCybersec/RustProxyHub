import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as mockBackend from '@/lib/mock-backend'
import { providerOrder, useStore } from '@/store'
import { makeOverview } from './factories'

vi.mock('@/lib/mock-backend', () => ({
  invoke: vi.fn(),
  providerOrder: ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai'],
}))

const mockedInvoke = vi.mocked(mockBackend.invoke)

describe('store runtime flow', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    mockedInvoke.mockReset()
  })

  it('surfaces refresh errors without crashing state', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('overview offline'))

    const store = useStore()
    await store.refreshOverview()

    expect(store.overview).toBeNull()
    expect(store.error).toBe('overview offline')
    expect(store.isRefreshing).toBe(false)
  })

  it('keeps browser provider ordering and default browser prefs', () => {
    const store = useStore()

    expect(providerOrder).toEqual(['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai'])
    expect(store.browserPrefs).toEqual({
      qwen: 'msedge',
      deepseek: 'msedge',
      kimi: 'msedge',
      chatgpt: 'msedge',
      gemini: 'msedge',
      mistral: 'msedge',
      zai: 'msedge',
    })
  })

  it('blocks workbench requests when runtime preflight failed', async () => {
    const store = useStore()
    store.overview = makeOverview({
      runtime: {
        edge_available: true,
        single_runner_ready: false,
        issues: ['Bundled node.exe not found in Tauri resources.'],
      },
    })

    await store.runWorkbench()

    expect(store.error).toBe('Bundled node.exe not found in Tauri resources.')
    expect(mockedInvoke).not.toHaveBeenCalled()
  })

  it('adds a qwen account and refreshes overview counts', async () => {
    const store = useStore()
    store.overview = makeOverview()
    store.qwenEmail = 'pilot@example.com'

    mockedInvoke.mockImplementation(async (command) => {
      if (command === 'add_qwen_account') {
        return [
          {
            id: 'acct-1',
            email: 'pilot@example.com',
            has_password: true,
            created_at: '2026-06-09T00:00:00Z',
          },
        ]
      }

      if (command === 'dashboard_overview') {
        return makeOverview({ qwen_account_count: 1 })
      }

      throw new Error(`unexpected invoke: ${String(command)}`)
    })

    await store.addQwenAccount('secret')

    expect(mockedInvoke).toHaveBeenCalledWith('add_qwen_account', {
      request: {
        email: 'pilot@example.com',
        password: 'secret',
      },
    })
    expect(store.qwenAccounts).toHaveLength(1)
    expect(store.overview?.qwen_account_count).toBe(1)
    expect(store.qwenEmail).toBe('')
  })
})
