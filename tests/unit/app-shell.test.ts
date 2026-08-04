import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import App from '@/App.vue'
import { makeOverview } from './factories'

// windowControls spies are driven via two paths depending on which invoke lands:
//   a) vi.mock('@tauri-apps/api/core') intercepts → mockedInvoke bridges to spies
//   b) real core.js is used (relative import from window.js) → __TAURI_INTERNALS__.invoke bridges
// Either path lands on the same windowControls object.
const windowControls = vi.hoisted(() => ({
  minimize: vi.fn(),
  toggleMaximize: vi.fn(),
  close: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => windowControls,
}))

const mockedInvoke = vi.mocked(invoke)

function mountApp() {
  const pinia = createPinia()
  setActivePinia(pinia)
  return mount(App, {
    global: { plugins: [pinia] },
  })
}

describe('app shell', () => {
  beforeEach(() => {
    window.localStorage.clear()

    // Provide the metadata getCurrentWindow() reads from the real @tauri-apps/api/window.js.
    // Also provide __TAURI_INTERNALS__.invoke so the real core.js invoke (imported relatively
    // inside window.js) bridges to windowControls spies even if vi.mock of core doesn't
    // intercept that relative-path import.
    ;(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      invoke: async (cmd: string) => {
        if (cmd === 'plugin:window|minimize') {
          windowControls.minimize()
          return
        }
        if (cmd === 'plugin:window|toggleMaximize') {
          windowControls.toggleMaximize()
          return
        }
        if (cmd === 'plugin:window|close') {
          windowControls.close()
          return
        }
        // other window plugin commands: no-op in tests
      },
    }

    mockedInvoke.mockReset()
    mockedInvoke.mockImplementation(async (command: unknown) => {
      if (command === 'dashboard_overview') return makeOverview()
      if (command === 'list_qwen_accounts') return []
      // bridge for path (a): vi.mock intercepted the core invoke from window.js
      if (command === 'plugin:window|minimize') {
        windowControls.minimize()
        return
      }
      if (command === 'plugin:window|toggleMaximize') {
        windowControls.toggleMaximize()
        return
      }
      if (command === 'plugin:window|close') {
        windowControls.close()
        return
      }
      throw new Error(`unexpected invoke: ${String(command)}`)
    })

    windowControls.minimize.mockReset()
    windowControls.toggleMaximize.mockReset()
    windowControls.close.mockReset()
  })

  afterEach(() => {
    // Clean up so the Tauri flag doesn't leak to other test files
    delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
  })

  it('renders the new shell in portuguese with brand and default tab', async () => {
    const wrapper = mountApp()
    await flushPromises()

    // brand always present in global-nav
    expect(wrapper.text()).toContain('RustProxyHub')
    // pt-BR default locale: app.headerTabs.overview = 'Início'
    expect(wrapper.text()).toContain('Início')
  })

  it('switches active tab label when a tab is clicked', async () => {
    const wrapper = mountApp()
    await flushPromises()

    // 'overview' tab is active by default
    expect(wrapper.get('.tab.active').text()).toBe('Início')

    // click the workbench tab (pt-BR app.headerTabs.workbench = 'Config')
    const configTab = wrapper.findAll('.tab').find(t => t.text() === 'Config')
    expect(configTab).toBeTruthy()
    await configTab!.trigger('click')

    expect(wrapper.get('.tab.active').text()).toBe('Config')
  })

  it('calls mocked Tauri window controls', async () => {
    const wrapper = mountApp()
    await flushPromises()

    // window controls only render when __TAURI_INTERNALS__ is present (set in beforeEach)
    expect(wrapper.find('.win-controls').exists()).toBe(true)

    // windowAction is async (dynamic import inside); flush after each click so the first
    // import resolves and is cached before the next trigger fires.
    await wrapper.get('button[aria-label="Minimize"]').trigger('click')
    await flushPromises()
    await wrapper.get('button[aria-label="Maximize"]').trigger('click')
    await flushPromises()
    await wrapper.get('button[aria-label="Close"]').trigger('click')
    await flushPromises()

    expect(windowControls.minimize).toHaveBeenCalledTimes(1)
    expect(windowControls.toggleMaximize).toHaveBeenCalledTimes(1)
    expect(windowControls.close).toHaveBeenCalledTimes(1)
  })
})
