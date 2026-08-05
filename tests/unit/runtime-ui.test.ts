import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it } from 'vitest'
import HubHeader from '@/components/dashboard/HubHeader.vue'
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
    const store = useStore()
    store.overview = makeOverview({
      runtime: {
        browser_available: false,
        single_runner_ready: false,
        issues: [
          'Bundled node.exe not found in Tauri resources.',
          'No supported browser found. Install Microsoft Edge, Google Chrome, or Chromium to run browser-backed providers.',
        ],
      },
    })

    const wrapper = mount(HubHeader, {
      global: { plugins: [pinia] },
    })

    // eyebrow shows pending state when single_runner_ready is false
    expect(wrapper.text()).toContain('preflight pendente')
    // issue strings surface in the .banner.banner-error list
    expect(wrapper.text()).toContain('Bundled node.exe not found in Tauri resources.')
    expect(wrapper.text()).toContain(
      'No supported browser found. Install Microsoft Edge, Google Chrome, or Chromium to run browser-backed providers.'
    )
  })

  it('shows a runtime error banner in workbench when runtime is not ready', () => {
    const store = useStore()
    store.overview = makeOverview({
      runtime: {
        browser_available: true,
        single_runner_ready: false,
        issues: ['Bundled node.exe not found in Tauri resources.'],
      },
    })

    const wrapper = mount(WorkbenchPanel, {
      global: { plugins: [pinia] },
    })

    // guard banner shows the first runtime issue (not a hard-disabled panel)
    expect(wrapper.get('.banner.banner-error').text()).toContain('Bundled node.exe not found in Tauri resources.')
    // run button is present; store validates before firing — not disabled at UI level
    expect(wrapper.find('.btn.btn-primary').exists()).toBe(true)
  })

})
