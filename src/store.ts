import { acceptHMRUpdate, defineStore } from 'pinia'
import { buildClaudeSetup, buildKiloSetup, buildPiSetup, type AgentSetupKind } from '@/lib/agent-setups'
import { invoke, providerOrder } from '@/lib/backend'
import { DEFAULT_LOCALE, defaultWorkbenchPrompt, translate, type UiLocale } from '@/lib/ui-i18n'
import type { UnlistenFn } from '@tauri-apps/api/event'
import type {
  BrowserPrefs,
  DashboardOverview,
  ProviderDetails,
  ProviderLogs,
  ProviderName,
  ProviderOverview,
  QwenAccountSummary,
} from '@/lib/types'

export { providerOrder }

const localeStorageKey = 'rustproxyhub.locale'

function loadLocale(): UiLocale {
  if (typeof window === 'undefined') return DEFAULT_LOCALE
  const stored = window.localStorage.getItem(localeStorageKey)
  return stored === 'en' || stored === 'pt-BR' ? stored : DEFAULT_LOCALE
}

const defaultBrowserPrefs: BrowserPrefs = {
  qwen: 'msedge',
  deepseek: 'msedge',
  kimi: 'msedge',
  chatgpt: 'msedge',
  gemini: 'msedge',
  mistral: 'msedge',
  zai: 'msedge',
  meta: 'msedge',
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
    locale: loadLocale() as UiLocale,
    version: '1.0.0',
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
    unlistenDashboard: null as UnlistenFn | null,
    qwenEmail: '',
    workbenchModel: '',
    workbenchPrompt: defaultWorkbenchPrompt(loadLocale()),
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
      const models = providers.flatMap((provider) =>
        provider.models.map((model) => formatHubModel(provider.name, model)),
      )
      return Array.from(new Set(models))
    },

    filteredQwenAccounts(): QwenAccountSummary[] {
      const query = this.searchQuery.trim().toLowerCase()
      if (!query) return this.qwenAccounts
      return this.qwenAccounts.filter((account) =>
        [account.email, account.id, account.created_at ?? ''].join(' ').toLowerCase().includes(query),
      )
    },

    activeProviderDetails(state): ProviderDetails | null {
      return state.activeDrawer ? state.providerDetails[state.activeDrawer] ?? null : null
    },

    activeProviderLogs(state): string[] {
      return state.activeDrawer ? state.providerLogs[state.activeDrawer] ?? [] : []
    },

    openLoginCount(state): number {
      return (
        (state.overview?.open_provider_login_sessions.length ?? 0) +
        (state.overview?.open_qwen_account_login_sessions.length ?? 0)
      )
    },

    localeCode(state): string {
      return state.locale === 'en' ? 'en-US' : 'pt-BR'
    },

    runtimeReady(state): boolean {
      return state.overview?.runtime.single_runner_ready ?? false
    },

    runtimeIssues(state): string[] {
      return state.overview?.runtime.issues ?? []
    },
  },

  actions: {
    t(key: string, params?: Record<string, string | number>) {
      return translate(this.locale, key, params)
    },

    setLocale(locale: UiLocale) {
      const previousLocale = this.locale
      const previousDefaultPrompt = defaultWorkbenchPrompt(previousLocale)
      this.locale = locale
      window.localStorage.setItem(localeStorageKey, locale)
      if (!this.workbenchPrompt.trim() || this.workbenchPrompt === previousDefaultPrompt) {
        this.workbenchPrompt = defaultWorkbenchPrompt(locale)
      }
    },

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

    async refreshOverview(silent = false) {
      if (this.isRefreshing) return
      this.isRefreshing = true
      try {
        if (!silent) this.notice = ''
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

      // Real Tauri: subscribe to push events from Rust for zero-latency dashboard updates.
      // Mock/dev mode: falls back to polling below.
      if ('__TAURI_INTERNALS__' in window) {
        try {
          const { listen } = await import('@tauri-apps/api/event')
          this.unlistenDashboard = await listen<DashboardOverview>('dashboard:update', (event) => {
            this.overview = event.payload
            this.syncWorkbenchModel()
          })
        } catch {
          // not in a Tauri context — polling fallback handles it
        }
      }

      // Fallback poll: drives mock-backend telemetry and covers gaps in SSE (e.g. log rotation).
      this.refreshTimer = window.setInterval(() => {
        if (!this.unlistenDashboard) void this.refreshOverview(true)
        if (this.activeDrawer) void this.refreshProviderLogs(this.activeDrawer).catch(() => {})
      }, 3000)
    },

    disposeApp() {
      if (this.refreshTimer != null) {
        window.clearInterval(this.refreshTimer)
        this.refreshTimer = null
      }
      if (this.unlistenDashboard != null) {
        this.unlistenDashboard()
        this.unlistenDashboard = null
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

    async addQwenAccount(password: string) {
      const email = this.qwenEmail.trim()
      if (!email) {
        this.error = this.t('store.emailRequired')
        return
      }
      // ponytail: password passed as arg, never stored in reactive state.
      await this.runTask('qwen-account:add', async () => {
        this.qwenAccounts = await invoke<QwenAccountSummary[]>('add_qwen_account', {
          request: { email, password },
        })
        this.qwenEmail = ''
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
          request: { provider, browser: this.browserPrefs[provider] },
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
          request: { account_id: accountId, browser: this.browserPrefs.qwen },
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
      const setup =
        agent === 'pi'
          ? buildPiSetup(provider)
          : agent === 'kilo'
            ? buildKiloSetup(provider)
            : buildClaudeSetup(provider)
      await copyText(setup.content)
      this.error = ''
      this.notice = this.t('store.copiedSetup', {
        agent: agent === 'pi' ? 'Pi' : agent === 'kilo' ? 'Kilo' : 'Claude',
        provider: providerName,
        target: setup.target,
      })
    },

    async runWorkbench() {
      const model = this.workbenchModel.trim()
      const prompt = this.workbenchPrompt.trim()
      if (!this.runtimeReady) {
        this.error = this.runtimeIssues[0] ?? this.t('store.runtimeBlocking')
        return
      }
      if (this.openLoginCount > 0) {
        this.error = this.t('store.closeLoginBeforeWorkbench')
        return
      }
      if (!this.overview?.hub.running) {
        this.error = this.t('store.startHubBeforeWorkbench')
        return
      }
      if (!model) {
        this.error = this.t('store.chooseModel')
        return
      }
      if (!prompt) {
        this.error = this.t('store.promptRequired')
        return
      }
      await this.runTask('workbench:run', async () => {
        const response = await invoke<unknown>('run_workbench_request', {
          request: { model, prompt, web_search: this.workbenchWebSearch },
        })
        this.workbenchResponse = JSON.stringify(
          response ?? { error: this.t('store.nullWorkbenchResponse') },
          null,
          2,
        )
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
