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

  it('renders remade shell in portuguese', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.text()).toContain('Central de comando do RustProxyHub')
    expect(wrapper.text()).toContain('ATUALIZAR PAINEL')
    expect(wrapper.text()).toContain('ATIVIDADE')
    expect(wrapper.text()).toContain('CREDITOS')
  })

  it('opens workbench from persistent help control', async () => {
    const wrapper = mountApp()
    await flushPromises()

    expect(wrapper.get('.tab.active').text()).toBe('Todos')
    await wrapper.get('button[aria-label="Ajuda"]').trigger('click')

    expect(wrapper.get('.tab.active').text()).toBe('Lab')
  })

  it('calls mocked Tauri window controls', async () => {
    const wrapper = mountApp()
    await flushPromises()

    await wrapper.get('button[aria-label="Minimizar"]').trigger('click')
    await wrapper.get('button[aria-label="Maximizar"]').trigger('click')
    await wrapper.get('button[aria-label="Fechar"]').trigger('click')

    expect(windowControls.minimize).toHaveBeenCalledTimes(1)
    expect(windowControls.toggleMaximize).toHaveBeenCalledTimes(1)
    expect(windowControls.close).toHaveBeenCalledTimes(1)
  })
})
