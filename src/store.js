import { invoke } from '@tauri-apps/api/core';
import { acceptHMRUpdate, defineStore } from 'pinia';
const versionString = import.meta.env.MODE === 'development' ? `${import.meta.env.VITE_APP_VERSION}-dev` : import.meta.env.VITE_APP_VERSION;
export const providerOrder = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini'];
const defaultBrowserPrefs = {
    qwen: 'msedge',
    deepseek: 'msedge',
    kimi: 'msedge',
    chatgpt: 'msedge',
    gemini: 'msedge',
};
function describeError(error) {
    return error instanceof Error ? error.message : String(error);
}
function formatHubModel(provider, model) {
    return model.startsWith(`${provider}:`) ? model : `${provider}:${model}`;
}
export const useStore = defineStore('main', {
    state: () => ({
        debug: import.meta.env.MODE === 'development',
        version: versionString,
        isInitialized: false,
        isRefreshing: false,
        error: '',
        overview: null,
        providerDetails: {},
        providerLogs: {},
        qwenAccounts: [],
        searchQuery: '',
        activeDrawer: null,
        browserPrefs: structuredClone(defaultBrowserPrefs),
        busy: {},
        refreshTimer: null,
        qwenEmail: '',
        qwenPassword: '',
        workbenchModel: '',
        workbenchPrompt: 'Say hello from RustProxyHub and report which provider answered.',
        workbenchWebSearch: false,
        workbenchResponse: '',
    }),
    getters: {
        providersByName: (state) => {
            const entries = state.overview?.providers ?? [];
            return entries.reduce((accumulator, provider) => {
                accumulator[provider.name] = provider;
                return accumulator;
            }, {});
        },
        filteredProviders() {
            const providers = this.overview?.providers ?? [];
            const query = this.searchQuery.trim().toLowerCase();
            if (!query)
                return providers;
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
                    .toLowerCase();
                return haystack.includes(query);
            });
        },
        hubModelOptions() {
            const providers = this.overview?.providers ?? [];
            const models = providers.flatMap((provider) => provider.models.map((model) => formatHubModel(provider.name, model)));
            return Array.from(new Set(models));
        },
        filteredQwenAccounts() {
            const query = this.searchQuery.trim().toLowerCase();
            if (!query)
                return this.qwenAccounts;
            return this.qwenAccounts.filter((account) => {
                return [account.email, account.id, account.created_at ?? ''].join(' ').toLowerCase().includes(query);
            });
        },
        activeProviderDetails(state) {
            return state.activeDrawer ? state.providerDetails[state.activeDrawer] ?? null : null;
        },
        activeProviderLogs(state) {
            return state.activeDrawer ? state.providerLogs[state.activeDrawer] ?? [] : [];
        },
        openLoginCount(state) {
            return (state.overview?.open_provider_login_sessions.length ?? 0) + (state.overview?.open_qwen_account_login_sessions.length ?? 0);
        },
    },
    actions: {
        setBusy(key, value) {
            this.busy[key] = value;
        },
        setSearchQuery(value) {
            this.searchQuery = value;
        },
        syncWorkbenchModel() {
            const available = this.hubModelOptions;
            if (!available.length)
                return;
            if (!this.workbenchModel || !available.includes(this.workbenchModel)) {
                this.workbenchModel = available[0];
            }
        },
        async runTask(key, task) {
            this.setBusy(key, true);
            this.error = '';
            try {
                return await task();
            }
            catch (error) {
                this.error = describeError(error);
                throw error;
            }
            finally {
                this.setBusy(key, false);
            }
        },
        async refreshOverview() {
            if (this.isRefreshing)
                return;
            this.isRefreshing = true;
            try {
                this.overview = await invoke('dashboard_overview');
                this.syncWorkbenchModel();
            }
            catch (error) {
                this.error = describeError(error);
            }
            finally {
                this.isRefreshing = false;
            }
        },
        async refreshProviderDetails(provider) {
            this.providerDetails[provider] = await invoke('provider_details', { provider });
        },
        async refreshProviderLogs(provider) {
            const response = await invoke('provider_logs', { provider });
            this.providerLogs[provider] = response.entries;
        },
        async loadQwenAccounts() {
            this.qwenAccounts = await invoke('list_qwen_accounts');
        },
        async initApp() {
            if (this.isInitialized)
                return;
            this.isInitialized = true;
            await this.refreshOverview();
            await this.loadQwenAccounts();
            this.refreshTimer = window.setInterval(() => {
                void this.refreshOverview();
            }, 4000);
        },
        disposeApp() {
            if (this.refreshTimer != null) {
                window.clearInterval(this.refreshTimer);
                this.refreshTimer = null;
            }
        },
        async openProviderDrawer(provider) {
            this.activeDrawer = provider;
            await this.runTask(`drawer:${provider}`, async () => {
                await Promise.all([this.refreshProviderDetails(provider), this.refreshProviderLogs(provider)]);
            });
        },
        closeProviderDrawer() {
            this.activeDrawer = null;
        },
        async addQwenAccount() {
            const email = this.qwenEmail.trim();
            if (!email) {
                this.error = 'Email is required.';
                return;
            }
            await this.runTask('qwen-account:add', async () => {
                this.qwenAccounts = await invoke('add_qwen_account', {
                    request: {
                        email,
                        password: this.qwenPassword,
                    },
                });
                this.qwenEmail = '';
                this.qwenPassword = '';
                await this.refreshOverview();
            });
        },
        async removeQwenAccount(accountId) {
            await this.runTask(`qwen-account:remove:${accountId}`, async () => {
                this.qwenAccounts = await invoke('remove_qwen_account', { accountId });
                await this.refreshOverview();
            });
        },
        async startProviderLogin(provider) {
            await this.runTask(`login:start:${provider}`, async () => {
                await invoke('start_provider_login_session', {
                    request: {
                        provider,
                        browser: this.browserPrefs[provider],
                    },
                });
                await this.refreshOverview();
            });
        },
        async stopProviderLogin(provider) {
            await this.runTask(`login:stop:${provider}`, async () => {
                await invoke('stop_provider_login_session', { provider });
                await this.refreshOverview();
            });
        },
        async startQwenAccountLogin(accountId) {
            await this.runTask(`login:qwen-account:start:${accountId}`, async () => {
                await invoke('start_qwen_account_login_session', {
                    request: {
                        account_id: accountId,
                        browser: this.browserPrefs.qwen,
                    },
                });
                await this.refreshOverview();
            });
        },
        async stopQwenAccountLogin(accountId) {
            await this.runTask(`login:qwen-account:stop:${accountId}`, async () => {
                await invoke('stop_qwen_account_login_session', { account_id: accountId });
                await this.refreshOverview();
            });
        },
        async runWorkbench() {
            const model = this.workbenchModel.trim();
            const prompt = this.workbenchPrompt.trim();
            if (!model) {
                this.error = 'Choose a prefixed hub model first.';
                return;
            }
            if (!prompt) {
                this.error = 'Prompt is required.';
                return;
            }
            await this.runTask('workbench:run', async () => {
                const response = await invoke('run_workbench_request', {
                    request: {
                        model,
                        prompt,
                        web_search: this.workbenchWebSearch,
                    },
                });
                this.workbenchResponse = JSON.stringify(response, null, 2);
                await this.refreshOverview();
            });
        },
        isBusy(key) {
            return !!this.busy[key];
        },
    },
});
if (import.meta.hot) {
    import.meta.hot.accept(acceptHMRUpdate(useStore, import.meta.hot));
}
