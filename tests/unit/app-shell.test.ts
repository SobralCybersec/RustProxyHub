import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke } from '@tauri-apps/api/core'
import App from '@/App.vue'
import { makeOverview } from './factories'

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
    mockedInvoke.mockReset()
    mockedInvoke.mockImplementation(async command => {
      if (command === 'dashboard_overview') return makeOverview()
      if (command === 'list_qwen_accounts') return []
      throw new Error(`unexpected invoke: ${String(command)}`)
    })
    windowControls.minimize.mockReset()
    windowControls.toggleMaximize.mockReset()
    windowControls.close.mockReset()
  })

  it('shows first-run tutorial and stores dismissed state', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.text()).toContain('Runtime Preflight')
    await wrapper.get('button.primary-button').trigger('click')
    await wrapper.get('button.primary-button').trigger('click')
    await wrapper.get('button.primary-button').trigger('click')

    expect(window.localStorage.getItem('rustproxyhub:tutorial-complete')).toBe('1')
  })

  it('reopens tutorial from persistent help control', async () => {
    window.localStorage.setItem('rustproxyhub:tutorial-complete', '1')
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.text()).not.toContain('Runtime Preflight')
    await wrapper.get('button[aria-label="Open guide"]').trigger('click')

    expect(wrapper.text()).toContain('Runtime Preflight')
  })

  it('calls mocked Tauri window controls', async () => {
    window.localStorage.setItem('rustproxyhub:tutorial-complete', '1')
    const wrapper = mountApp()
    await flushPromises()

    await wrapper.get('button[aria-label="Minimize"]').trigger('click')
    await wrapper.get('button[aria-label="Maximize or restore"]').trigger('click')
    await wrapper.get('button[aria-label="Close"]').trigger('click')

    expect(windowControls.minimize).toHaveBeenCalledTimes(1)
    expect(windowControls.toggleMaximize).toHaveBeenCalledTimes(1)
    expect(windowControls.close).toHaveBeenCalledTimes(1)
  })
})
