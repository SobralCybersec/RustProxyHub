import { invoke } from '@tauri-apps/api/core'
import { acceptHMRUpdate, defineStore } from 'pinia'
import type {
  DashboardSnapshot,
  ProviderName,
  ServiceConfig,
  ServiceName,
  ServiceSnapshot,
} from '@/lib/types'

const versionString =
  import.meta.env.MODE === 'development' ? `${import.meta.env.VITE_APP_VERSION}-dev` : import.meta.env.VITE_APP_VERSION

const providerNames: ProviderName[] = ['qwen', 'deepseek', 'kimi']
const serviceNames: ServiceName[] = ['hub', 'qwen', 'deepseek', 'kimi']

const defaultServiceConfigs: Record<ServiceName, ServiceConfig> = {
  hub: {
    port: 3100,
    apiKey: '',
    browser: 'chromium',
    headless: true,
  },
  qwen: {
    port: 3000,
    apiKey: '',
    browser: 'chromium',
    headless: true,
  },
  deepseek: {
    port: 3001,
    apiKey: '',
    browser: 'chromium',
    headless: true,
  },
  kimi: {
    port: 3002,
    apiKey: '',
    browser: 'chromium',
    headless: true,
  },
}

type BusyMap = Record<string, boolean>

function describeError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

export const useStore = defineStore('main', {
  state: () => ({
    debug: import.meta.env.MODE === 'development',
    version: versionString,
    isInitialized: false,
    isRefreshing: false,
    error: '',
    dashboard: null as DashboardSnapshot | null,
    qwenEmail: '',
    qwenPassword: '',
    serviceConfigs: structuredClone(defaultServiceConfigs) as Record<ServiceName, ServiceConfig>,
    busy: {} as BusyMap,
    refreshTimer: null as number | null,
    workbenchService: 'hub' as ServiceName,
    workbenchModel: '',
    workbenchPrompt: 'Say hello from RustProxyHub and report which provider answered.',
    workbenchResponse: '',
  }),

  getters: {
    serviceByName: (state) => {
      return (provider: ServiceName): ServiceSnapshot | undefined =>
        state.dashboard?.services.find((service) => service.provider === provider)
    },
    activeServiceCount: (state) => state.dashboard?.services.filter((service) => service.running).length ?? 0,
    qwenService: (state) => state.dashboard?.services.find((service) => service.provider === 'qwen') ?? null,
    hubService: (state) => state.dashboard?.services.find((service) => service.provider === 'hub') ?? null,
    qwenAccounts: (state) => state.dashboard?.qwen_accounts ?? [],
    availableWorkbenchModels(): string[] {
      const service = this.serviceByName(this.workbenchService)
      const ids = service?.models.map((model) => model.id) ?? []
      return Array.from(new Set(ids))
    },
  },

  actions: {
    setBusy(key: string, value: boolean) {
      this.busy[key] = value
    },

    setWorkbenchService(service: ServiceName) {
      this.workbenchService = service
      this.syncWorkbenchModel()
    },

    syncWorkbenchModel() {
      const available = this.availableWorkbenchModels
      if (!available.length) return
      if (!this.workbenchModel || !available.includes(this.workbenchModel)) {
        this.workbenchModel = available[0]
      }
    },

    async runTask<T>(key: string, task: () => Promise<T>): Promise<T> {
      this.setBusy(key, true)
      this.error = ''
      try {
        return await task()
      } catch (error) {
        this.error = describeError(error)
        throw error
      } finally {
        this.setBusy(key, false)
      }
    },

    async refreshSnapshot() {
      if (this.isRefreshing) return
      this.isRefreshing = true
      try {
        const snapshot = await invoke<DashboardSnapshot>('app_snapshot')
        this.dashboard = snapshot
        for (const service of snapshot.services) {
          if (service.port != null) {
            this.serviceConfigs[service.provider].port = service.port
          }
        }
        this.syncWorkbenchModel()
      } catch (error) {
        this.error = describeError(error)
      } finally {
        this.isRefreshing = false
      }
    },

    async initApp() {
      if (this.isInitialized) return
      this.isInitialized = true
      await this.refreshSnapshot()
      this.refreshTimer = window.setInterval(() => {
        void this.refreshSnapshot()
      }, 3500)
    },

    disposeApp() {
      if (this.refreshTimer != null) {
        window.clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
    },

    buildStartRequest(service: ServiceName) {
      const config = this.serviceConfigs[service]
      return {
        provider: service,
        port: config.port,
        apiKey: config.apiKey,
        browser: config.browser,
        headless: config.headless,
        upstreams:
          service === 'hub'
            ? providerNames.map((provider) => ({
                provider,
                port: this.serviceConfigs[provider].port,
                apiKey: this.serviceConfigs[provider].apiKey,
              }))
            : undefined,
      }
    },

    async startService(service: ServiceName) {
      await this.runTask(`start:${service}`, async () => {
        await invoke<ServiceSnapshot>('start_service', {
          request: this.buildStartRequest(service),
        })
        await this.refreshSnapshot()
      })
    },

    async stopService(service: ServiceName) {
      await this.runTask(`stop:${service}`, async () => {
        await invoke<ServiceSnapshot>('stop_service', { provider: service })
        await this.refreshSnapshot()
      })
    },

    async startStack() {
      await this.runTask('stack:start', async () => {
        for (const service of serviceNames) {
          await invoke<ServiceSnapshot>('start_service', {
            request: this.buildStartRequest(service),
          })
        }
        await this.refreshSnapshot()
      })
    },

    async stopStack() {
      await this.runTask('stack:stop', async () => {
        for (const service of [...serviceNames].reverse()) {
          await invoke<ServiceSnapshot>('stop_service', { provider: service })
        }
        await this.refreshSnapshot()
      })
    },

    async addQwenAccount() {
      const email = this.qwenEmail.trim()
      if (!email) {
        this.error = 'Email is required.'
        return
      }

      await this.runTask('account:add', async () => {
        await invoke('add_qwen_account', {
          request: {
            email,
            password: this.qwenPassword,
          },
        })
        this.qwenEmail = ''
        this.qwenPassword = ''
        await this.refreshSnapshot()
      })
    },

    async addQwenAccountAndOpenLogin() {
      const email = this.qwenEmail.trim()
      if (!email) {
        this.error = 'Email is required.'
        return
      }

      await this.runTask('account:add-open-login', async () => {
        await invoke('add_qwen_account', {
          request: {
            email,
            password: this.qwenPassword,
          },
        })
        await this.refreshSnapshot()

        const account = this.qwenAccounts.find((entry) => entry.email.toLowerCase() === email.toLowerCase())
        if (!account) {
          throw new Error('Account was saved but could not be found for login launch.')
        }

        await invoke('start_qwen_login_session', {
          request: {
            accountId: account.id,
            browser: this.serviceConfigs.qwen.browser,
          },
        })

        this.qwenEmail = ''
        this.qwenPassword = ''
        await this.refreshSnapshot()
      })
    },

    async removeQwenAccount(accountId: string) {
      await this.runTask(`account:remove:${accountId}`, async () => {
        await invoke('remove_qwen_account', { accountId })
        await this.refreshSnapshot()
      })
    },

    async startQwenLogin(accountId: string) {
      await this.runTask(`login:start:${accountId}`, async () => {
        await invoke('start_qwen_login_session', {
          request: {
            accountId,
            browser: this.serviceConfigs.qwen.browser,
          },
        })
        await this.refreshSnapshot()
      })
    },

    async stopQwenLogin(accountId: string) {
      await this.runTask(`login:stop:${accountId}`, async () => {
        await invoke('stop_qwen_login_session', { accountId })
        await this.refreshSnapshot()
      })
    },

    async startProviderLogin(provider: ProviderName) {
      await this.runTask(`provider-login:start:${provider}`, async () => {
        await invoke('start_provider_login_session', {
          request: {
            provider,
            browser: this.serviceConfigs[provider].browser,
          },
        })
        await this.refreshSnapshot()
      })
    },

    async stopProviderLogin(provider: ProviderName) {
      await this.runTask(`provider-login:stop:${provider}`, async () => {
        await invoke('stop_provider_login_session', { provider })
        await this.refreshSnapshot()
      })
    },

    async runWorkbench() {
      const model = this.workbenchModel.trim()
      const prompt = this.workbenchPrompt.trim()
      if (!model) {
        this.error = 'Choose or type a model first.'
        return
      }
      if (!prompt) {
        this.error = 'Prompt is required.'
        return
      }

      await this.runTask('workbench:run', async () => {
        const response = await invoke<unknown>('run_workbench_request', {
          request: {
            service: this.workbenchService,
            model,
            prompt,
          },
        })
        this.workbenchResponse = JSON.stringify(response, null, 2)
      })
    },

    isBusy(key: string) {
      return !!this.busy[key]
    },
  },
})

if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useStore, import.meta.hot))
}
