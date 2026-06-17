import { invoke } from '@tauri-apps/api/core'
import { acceptHMRUpdate, defineStore } from 'pinia'
import { buildClaudeSetup, buildPiSetup, type AgentSetupKind } from '@/lib/agent-setups'
import type {
  BrowserPrefs,
  DashboardOverview,
  ProviderDetails,
  ProviderLogs,
  ProviderName,
  ProviderOverview,
  QwenAccountSummary,
} from '@/lib/types'

const versionString =
  import.meta.env.MODE === 'development' ? `${import.meta.env.VITE_APP_VERSION}-dev` : import.meta.env.VITE_APP_VERSION

export const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral']

const defaultBrowserPrefs: BrowserPrefs = {
  qwen: 'msedge',
  deepseek: 'msedge',
  kimi: 'msedge',
  chatgpt: 'msedge',
  gemini: 'msedge',
  mistral: 'msedge',
}

type BusyMap = Record<string, boolean>

function describeError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatHubModel(provider: ProviderName, model: string) {
  return model.startsWith(`${provider}:`) ? model : `${provider}:${model}`
}

async function copyText(value: string) {
  if (navigator?.clipboard?.writeText) {
    await navigator.clipboard.writeText(value)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = value
  textarea.setAttribute('readonly', 'true')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  document.execCommand('copy')
  document.body.removeChild(textarea)
}

export const useStore = defineStore('main', {
  state: () => ({
    debug: import.meta.env.MODE === 'development',
    version: versionString,
    isInitialized: false,
    isRefreshing: false,
    error: '',
    notice: '',
    overview: null as DashboardOverview | null,
    providerDetails: {} as Partial<Record<ProviderName, ProviderDetails>>,
    providerLogs: {} as Partial<Record<ProviderName | 'hub', string[]>>,
    qwenAccounts: [] as QwenAccountSummary[],
    searchQuery: '',
    activeDrawer: null as ProviderName | null,
    browserPrefs: structuredClone(defaultBrowserPrefs) as BrowserPrefs,
    busy: {} as BusyMap,
    refreshTimer: null as number | null,
    qwenEmail: '',
    qwenPassword: '',
    workbenchModel: '',
    workbenchPrompt: 'Say hello from RustProxyHub and report which provider answered.',
    workbenchWebSearch: false,
    workbenchResponse: '',
  }),

  getters: {
    providersByName: (state) => {
      const entries = state.overview?.providers ?? []
      return entries.reduce(
        (accumulator, provider) => {
          accumulator[provider.name] = provider
          return accumulator
        },
        {} as Record<ProviderName, ProviderOverview>,
      )
    },

    filteredProviders(): ProviderOverview[] {
      const providers = this.overview?.providers ?? []
      const query = this.searchQuery.trim().toLowerCase()
      if (!query) return providers

      return providers.filter((provider) => {
        const haystack = [
          provider.name,
          provider.health_status,
          provider.login_state,
          provider.base_url,
          ...provider.models,
          provider.last_error ?? '',
        ]
          .join(' ')
          .toLowerCase()
        return haystack.includes(query)
      })
    },

    hubModelOptions(): string[] {
      const providers = this.overview?.providers ?? []
      const models = providers.flatMap((provider) => provider.models.map((model) => formatHubModel(provider.name, model)))
      return Array.from(new Set(models))
    },

    filteredQwenAccounts(): QwenAccountSummary[] {
      const query = this.searchQuery.trim().toLowerCase()
      if (!query) return this.qwenAccounts

      return this.qwenAccounts.filter((account) => {
        return [account.email, account.id, account.created_at ?? ''].join(' ').toLowerCase().includes(query)
      })
    },

    activeProviderDetails(state): ProviderDetails | null {
      return state.activeDrawer ? state.providerDetails[state.activeDrawer] ?? null : null
    },

    activeProviderLogs(state): string[] {
      return state.activeDrawer ? state.providerLogs[state.activeDrawer] ?? [] : []
    },

    openLoginCount(state): number {
      return (state.overview?.open_provider_login_sessions.length ?? 0) + (state.overview?.open_qwen_account_login_sessions.length ?? 0)
    },

    runtimeReady(state): boolean {
      return state.overview?.runtime.single_runner_ready ?? false
    },

    runtimeIssues(state): string[] {
      return state.overview?.runtime.issues ?? []
    },
  },

  actions: {
    setBusy(key: string, value: boolean) {
      this.busy[key] = value
    },

    setNotice(value: string) {
      this.notice = value
    },

    setSearchQuery(value: string) {
      this.searchQuery = value
    },

    syncWorkbenchModel() {
      const available = this.hubModelOptions
      if (!available.length) return
      if (!this.workbenchModel || !available.includes(this.workbenchModel)) {
        this.workbenchModel = available[0]
      }
    },

    async runTask<T>(key: string, task: () => Promise<T>): Promise<T> {
      this.setBusy(key, true)
      this.error = ''
      this.notice = ''
      try {
        return await task()
      } catch (error) {
        this.error = describeError(error)
        throw error
      } finally {
        this.setBusy(key, false)
      }
    },

    async refreshOverview() {
      if (this.isRefreshing) return
      this.isRefreshing = true
      try {
        this.notice = ''
        this.overview = await invoke<DashboardOverview>('dashboard_overview')
      } catch (error) {
        this.error = describeError(error)
      } finally {
        this.isRefreshing = false
      }
    },

    async refreshProviderDetails(provider: ProviderName) {
      this.providerDetails[provider] = await invoke<ProviderDetails>('provider_details', { provider })
    },

    async refreshProviderLogs(provider: ProviderName | 'hub') {
      const response = await invoke<ProviderLogs>('provider_logs', { provider })
      this.providerLogs[provider] = response.entries
    },

    async loadQwenAccounts() {
      this.qwenAccounts = await invoke<QwenAccountSummary[]>('list_qwen_accounts')
    },

    async initApp() {
      if (this.isInitialized) return
      this.isInitialized = true
      await this.refreshOverview()
      this.syncWorkbenchModel()
      await this.loadQwenAccounts()
    },

    disposeApp() {
      if (this.refreshTimer != null) {
        window.clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
    },

    async openProviderDrawer(provider: ProviderName) {
      this.activeDrawer = provider
      await this.runTask(`drawer:${provider}`, async () => {
        await Promise.all([this.refreshProviderDetails(provider), this.refreshProviderLogs(provider)])
      })
    },

    closeProviderDrawer() {
      this.activeDrawer = null
    },

    async addQwenAccount() {
      const email = this.qwenEmail.trim()
      if (!email) {
        this.error = 'Email is required.'
        return
      }

      await this.runTask('qwen-account:add', async () => {
        this.qwenAccounts = await invoke<QwenAccountSummary[]>('add_qwen_account', {
          request: {
            email,
            password: this.qwenPassword,
          },
        })
        this.qwenEmail = ''
        this.qwenPassword = ''
        await this.refreshOverview()
      })
    },

    async removeQwenAccount(accountId: string) {
      await this.runTask(`qwen-account:remove:${accountId}`, async () => {
        this.qwenAccounts = await invoke<QwenAccountSummary[]>('remove_qwen_account', { accountId })
        await this.refreshOverview()
      })
    },

    async startProviderLogin(provider: ProviderName) {
      await this.runTask(`login:start:${provider}`, async () => {
        await invoke<string[]>('start_provider_login_session', {
          request: {
            provider,
            browser: this.browserPrefs[provider],
          },
        })
        await this.refreshOverview()
      })
    },

    async stopProviderLogin(provider: ProviderName) {
      await this.runTask(`login:stop:${provider}`, async () => {
        await invoke<string[]>('stop_provider_login_session', { provider })
        await this.refreshOverview()
      })
    },

    async startQwenAccountLogin(accountId: string) {
      await this.runTask(`login:qwen-account:start:${accountId}`, async () => {
        await invoke<string[]>('start_qwen_account_login_session', {
          request: {
            account_id: accountId,
            browser: this.browserPrefs.qwen,
          },
        })
        await this.refreshOverview()
      })
    },

    async stopQwenAccountLogin(accountId: string) {
      await this.runTask(`login:qwen-account:stop:${accountId}`, async () => {
        await invoke<string[]>('stop_qwen_account_login_session', { account_id: accountId })
        await this.refreshOverview()
      })
    },

    async startService(name: string) {
      await this.runTask(`service:start:${name}`, async () => {
        await invoke('start_service', { name })
        await this.refreshOverview()
        this.syncWorkbenchModel()
      })
    },

    async copyAgentSetup(providerName: ProviderName, agent: AgentSetupKind) {
      const provider = this.providersByName[providerName]
      if (!provider) {
        this.error = `Provider ${providerName} is not loaded yet.`
        return
      }

      const setup = agent === 'pi' ? buildPiSetup(provider) : buildClaudeSetup(provider)
      await copyText(setup.content)
      this.error = ''
      this.notice = `${agent === 'pi' ? 'Pi' : 'Claude'} setup copied for ${providerName}. Target: ${setup.target}`
    },

    async runWorkbench() {
      const model = this.workbenchModel.trim()
      const prompt = this.workbenchPrompt.trim()
      if (!this.runtimeReady) {
        this.error = this.runtimeIssues[0] ?? 'Runtime preflight is blocking startup.'
        return
      }
      if (this.openLoginCount > 0) {
        this.error = 'Close active login session before running workbench request.'
        return
      }
      if (!this.overview?.hub.running) {
        this.error = 'Start hub service before running workbench request.'
        return
      }
      if (!model) {
        this.error = 'Choose a prefixed hub model first.'
        return
      }
      if (!prompt) {
        this.error = 'Prompt is required.'
        return
      }

      await this.runTask('workbench:run', async () => {
        const response = await invoke<unknown>('run_workbench_request', {
          request: {
            model,
            prompt,
            web_search: this.workbenchWebSearch,
          },
        })
        this.workbenchResponse = JSON.stringify(response ?? { error: 'Workbench returned null response.' }, null, 2)
        await this.refreshOverview()
        this.syncWorkbenchModel()
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
