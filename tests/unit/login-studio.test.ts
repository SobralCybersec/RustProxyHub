import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import { useStore } from '@/store'
import { makeOverview } from './factories'

describe('Qwen account manager', () => {
  let pinia: ReturnType<typeof createPinia>

  beforeEach(() => {
    pinia = createPinia()
    setActivePinia(pinia)
  })

  it('shows each stored account and opens account-specific login', async () => {
    const store = useStore()
    store.overview = makeOverview({ open_qwen_account_login_sessions: [] })
    store.qwenAccounts = [
      {
        id: 'account-1',
        email: 'one@example.test',
        has_password: true,
        created_at: null,
      },
    ]
    const startLogin = vi.spyOn(store, 'startQwenAccountLogin').mockResolvedValue()

    const wrapper = mount(LoginStudio, { global: { plugins: [pinia] } })
    expect(wrapper.text()).toContain('one@example.test')

    const accountCard = wrapper.find('.account-row')
    await accountCard.get('button.btn-primary').trigger('click')
    expect(startLogin).toHaveBeenCalledWith('account-1')
  })
})
