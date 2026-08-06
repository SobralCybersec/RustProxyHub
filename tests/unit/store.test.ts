import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as mockBackend from '@/lib/mock-backend'
import { providerOrder, useStore } from '@/store'
import { makeOverview } from './factories'

vi.mock('@/lib/mock-backend', () => ({
  invoke: vi.fn(),
  providerOrder: ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai', 'meta'],
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

    expect(providerOrder).toEqual(['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai', 'meta'])
    expect(store.browserPrefs).toEqual({
      qwen: 'msedge',
      deepseek: 'msedge',
      kimi: 'msedge',
      chatgpt: 'msedge',
      gemini: 'msedge',
      mistral: 'msedge',
      zai: 'msedge',
      meta: 'msedge',
    })
  })

  it('blocks workbench requests when runtime preflight failed', async () => {
    const store = useStore()
    store.overview = makeOverview({
      runtime: {
        browser_available: true,
        single_runner_ready: false,
        issues: ['Bundled node.exe not found in Tauri resources.'],
      },
    })

    await store.runWorkbench()

    expect(store.error).toBe('Bundled node.exe not found in Tauri resources.')
    expect(mockedInvoke).not.toHaveBeenCalled()
  })

  it('opens and closes manual login with browser preferences for every provider', async () => {
    const store = useStore()
    for (const provider of providerOrder) {
      mockedInvoke.mockResolvedValueOnce([])
      mockedInvoke.mockResolvedValueOnce(makeOverview())
      mockedInvoke.mockResolvedValueOnce([])
      mockedInvoke.mockResolvedValueOnce(makeOverview())

      await store.startProviderLogin(provider)
      await store.stopProviderLogin(provider)

      expect(mockedInvoke).toHaveBeenCalledWith('start_provider_login_session', {
        request: { provider, browser: 'msedge' },
      })
      expect(mockedInvoke).toHaveBeenCalledWith('stop_provider_login_session', { provider })
      expect(store.isBusy(`login:start:${provider}`)).toBe(false)
      expect(store.isBusy(`login:stop:${provider}`)).toBe(false)
    }
  })

  it('surfaces manual login connection errors and clears busy state', async () => {
    mockedInvoke.mockRejectedValueOnce(new Error('provider did not become ready'))
    const store = useStore()

    await expect(store.startProviderLogin('qwen')).rejects.toThrow('provider did not become ready')

    expect(store.error).toBe('provider did not become ready')
    expect(store.isBusy('login:start:qwen')).toBe(false)
  })
})
