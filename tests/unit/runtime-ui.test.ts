import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'
import { useStore } from '@/store'
import { makeOverview } from './factories'

describe('runtime diagnostics ui', () => {
  let pinia: ReturnType<typeof createPinia>

  beforeEach(() => {
    pinia = createPinia()
    setActivePinia(pinia)
  })

  it('shows degraded runtime details in the header', () => {
    const overview = makeOverview({
      runtime: {
        edge_available: false,
        single_runner_ready: false,
        issues: [
          'Bundled node.exe not found in Tauri resources.',
          'Microsoft Edge not found. Install Edge to run browser-backed providers.',
        ],
      },
    })

    const wrapper = mount(HubHeader, {
      props: { overview },
      global: { plugins: [pinia] },
    })

    expect(wrapper.text()).toContain('preflight do single-runner bloqueado')
    expect(wrapper.text()).toContain('Bundled node.exe not found in Tauri resources.')
    expect(wrapper.text()).toContain('Microsoft Edge not found. Install Edge to run browser-backed providers.')
    expect(wrapper.text()).toContain('"startup_mode": "manual"')
  })

  it('disables workbench controls until runtime is ready', () => {
    const store = useStore()
    store.overview = makeOverview({
      runtime: {
        edge_available: true,
        single_runner_ready: false,
        issues: ['Bundled node.exe not found in Tauri resources.'],
      },
    })

    const wrapper = mount(WorkbenchPanel, {
      global: { plugins: [pinia] },
    })

    expect(wrapper.get('.panel-alert').text()).toContain('Bundled node.exe not found in Tauri resources.')
    expect(wrapper.get('input[list="hub-model-options"]').element).toHaveProperty('disabled', true)
    expect(wrapper.get('input[type="checkbox"]').element).toHaveProperty('disabled', true)
    expect(wrapper.get('textarea').element).toHaveProperty('disabled', true)
    const runButton = wrapper.findAll('button').find(button => button.text().includes('rodar probe live'))
    expect(runButton).toBeTruthy()
    expect(runButton!.element).toHaveProperty('disabled', true)
  })

  it('renders qwen account actions from live store state', () => {
    const store = useStore()
    store.qwenAccounts = [
      {
        id: 'acct-1',
        email: 'pilot@example.com',
        has_password: true,
        created_at: '2026-06-09T00:00:00Z',
      },
    ]
    store.overview = makeOverview({
      open_qwen_account_login_sessions: ['acct-1'],
    })

    const wrapper = mount(QwenAccountsPanel, {
      global: { plugins: [pinia] },
    })

    expect(wrapper.text()).toContain('pilot@example.com')
    expect(wrapper.text()).toContain('perfil aberto')
    expect(wrapper.text()).toContain('abrir perfil')
    expect(wrapper.text()).toContain('remover')
  })
})
