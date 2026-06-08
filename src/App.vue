<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { computed, onBeforeUnmount, onMounted } from 'vue'
import type { ProviderName, ServiceName, ServiceSnapshot } from '@/lib/types'

const store = useStore()
const {
  dashboard,
  error,
  qwenService,
  hubService,
  activeServiceCount,
  qwenEmail,
  qwenPassword,
  serviceConfigs,
  workbenchService,
  workbenchModel,
  workbenchPrompt,
  workbenchResponse,
} = storeToRefs(store)

const serviceOrder: ServiceName[] = ['hub', 'qwen', 'deepseek', 'kimi']
const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi']

const serviceTitles: Record<ServiceName, string> = {
  hub: 'Unified API Hub',
  qwen: 'Qwen Stage 2',
  deepseek: 'DeepSeek Parity',
  kimi: 'Kimi Continue Loop',
}

const serviceDescriptions: Record<ServiceName, string> = {
  hub: 'One OpenAI-compatible gateway and one OpenAPI point for real integrations.',
  qwen: 'Account rotation, uploads, stop endpoint, multimodal routing, metrics, and watchdog.',
  deepseek: 'Browser-assisted DeepSeek parity proxy with reasoning and tool call streaming.',
  kimi: 'Kimi streaming proxy with pause detection and auto-continue recovery.',
}

const serviceMap = computed<Record<ServiceName, ServiceSnapshot | null>>(() => ({
  hub: store.serviceByName('hub') ?? null,
  qwen: store.serviceByName('qwen') ?? null,
  deepseek: store.serviceByName('deepseek') ?? null,
  kimi: store.serviceByName('kimi') ?? null,
}))

const qwenCooldowns = computed(() => {
  const cooldowns = qwenService.value?.admin_status?.cooldowns
  if (!cooldowns || typeof cooldowns !== 'object') return []
  return Object.entries(cooldowns as Record<string, { remaining_ms?: number; reason?: string }>).map(([id, value]) => ({
    id,
    remaining: Math.round((value?.remaining_ms ?? 0) / 1000),
    reason: value?.reason ?? 'Cooldown',
  }))
})

const hubProviderStatus = computed(() => {
  const source = hubService.value?.admin_status
  return Array.isArray(source) ? source : []
})

const sampleCurl = computed(() => {
  const service = hubService.value
  const base = service?.endpoints.base_url
  if (!base) return 'Launch the hub to get a live integration curl example.'
  const model = workbenchModel.value || service.models[0]?.id || 'qwen:qwen3.7-plus'
  const apiKey = serviceConfigs.value.hub.apiKey.trim()
  const authLine = apiKey ? `  -H "Authorization: Bearer ${apiKey}" \\\n` : ''
  return [
    `curl ${base}/v1/chat/completions \\`,
    authLine + '  -H "Content-Type: application/json" \\',
    `  -d '{"model":"${model}","messages":[{"role":"user","content":"Hello from RustProxyHub"}]}'`,
  ].join('\n')
})

const availableWorkbenchModels = computed(() => store.availableWorkbenchModels)

const loginGuides: Record<ProviderName, { title: string; summary: string; steps: string[]; note: string }> = {
  qwen: {
    title: 'Qwen browser session',
    summary: 'Legacy qwenproxy used saved credentials or manual browser login. Rust hub still needs that real session.',
    steps: [
      'Open global login for one default _default browser profile.',
      'For rotation, save an account below, then open that account login to persist its own profile.',
      'Leave password blank if you only want manual browser login.',
    ],
    note: 'Global login fixes default profile auth. Account login fixes rotation profiles.',
  },
  deepseek: {
    title: 'DeepSeek manual login',
    summary: 'Legacy deepsproxy only opened browser and waited until chat input was visible.',
    steps: [
      'Launch DeepSeek service.',
      'Open login and sign in on chat.deepseek.com.',
      'Wait until chat input is visible, then rerun probe.',
    ],
    note: 'If hub still says timeout waiting for chat input, profile is not fully logged in yet.',
  },
  kimi: {
    title: 'Kimi manual login',
    summary: 'Legacy kimiproxy also used visible browser login and saved that persistent profile.',
    steps: [
      'Launch Kimi service.',
      'Open login and finish sign in on www.kimi.com.',
      'Keep that saved profile for later requests.',
    ],
    note: 'Kimi login is same browser-state flow, but current saved session already works.',
  },
}

function providerLoginOpen(provider: ProviderName) {
  return dashboard.value?.provider_login_sessions.includes(provider) ?? false
}

function loginTone(provider: ProviderName) {
  return providerLoginOpen(provider) ? 'healthy' : 'idle'
}

function qwenAccountLoginOpen(accountId: string) {
  return dashboard.value?.open_login_sessions.includes(accountId) ?? false
}

function formatStarted(value: number | null) {
  if (!value) return 'n/a'
  return new Date(value * 1000).toLocaleString()
}

function statusTone(service: ServiceSnapshot | null) {
  if (!service?.running) return 'idle'
  if (service.health?.status === 'ok') return 'healthy'
  if (service.provider === 'hub' && service.health?.status === 'degraded') return 'degraded'
  return 'running'
}

function previewModels(service: ServiceSnapshot | null) {
  return service?.models.slice(0, 8) ?? []
}

function copyText(value: string | null | undefined) {
  if (!value) return
  void navigator.clipboard?.writeText(value)
}

function prettyJson(value: unknown) {
  return JSON.stringify(value, null, 2)
}

onMounted(() => {
  void store.initApp()
})

onBeforeUnmount(() => {
  store.disposeApp()
})
</script>

<template>
  <div class="shell">
    <div class="ambient ambient-a" />
    <div class="ambient ambient-b" />

    <main class="frame">
      <header class="hero panel">
        <div class="hero-copy">
          <p class="eyebrow">RustProxyHub Desktop</p>
          <h1>One desk for the full proxy stack and one API point for integrations.</h1>
          <p class="lede">
            Launch DeepSeek, Kimi, Qwen, and the unified hub together, open browser login windows,
            inspect live endpoints, and send real prompt checks without bouncing between terminals.
          </p>
        </div>

        <div class="hero-rail">
          <div class="hero-actions">
            <button class="primary-button" :disabled="store.isBusy('stack:start')" @click="store.startStack()">
              {{ store.isBusy('stack:start') ? 'Launching stack...' : 'Launch full stack' }}
            </button>
            <button class="secondary-button" :disabled="store.isBusy('stack:stop')" @click="store.stopStack()">
              {{ store.isBusy('stack:stop') ? 'Stopping stack...' : 'Stop full stack' }}
            </button>
            <button class="ghost-button" :disabled="store.isRefreshing" @click="store.refreshSnapshot()">
              {{ store.isRefreshing ? 'Refreshing...' : 'Refresh pulse' }}
            </button>
          </div>

          <div class="stat-stack">
            <div class="stat-pill">
              <span>Live services</span>
              <strong>{{ activeServiceCount }}</strong>
            </div>
            <div class="stat-pill">
              <span>Hub models</span>
              <strong>{{ hubService?.model_count ?? 0 }}</strong>
            </div>
            <div class="stat-pill">
              <span>Qwen accounts</span>
              <strong>{{ dashboard?.qwen_accounts.length ?? 0 }}</strong>
            </div>
            <div class="stat-pill">
              <span>Browser logins</span>
              <strong>{{ (dashboard?.provider_login_sessions.length ?? 0) + (dashboard?.open_login_sessions.length ?? 0) }}</strong>
            </div>
          </div>
        </div>
      </header>

      <p v-if="error" class="error-banner">{{ error }}</p>

      <section class="board">
        <div class="lane services-lane">
          <article
            v-for="serviceName in serviceOrder"
            :key="serviceName"
            class="panel service-panel"
            :data-provider="serviceName"
          >
            <div class="panel-top">
              <div>
                <p class="eyebrow">{{ serviceName }}</p>
                <h2>{{ serviceTitles[serviceName] }}</h2>
                <p class="panel-copy">{{ serviceDescriptions[serviceName] }}</p>
              </div>
              <span class="status-chip" :data-state="statusTone(serviceMap[serviceName])">
                {{ serviceMap[serviceName]?.running ? (serviceMap[serviceName]?.health?.status ?? 'running') : 'idle' }}
              </span>
            </div>

            <div class="control-grid" :class="{ compact: serviceName === 'hub' }">
              <label class="field">
                <span>Port</span>
                <input v-model.number="serviceConfigs[serviceName].port" type="number" min="1" max="65535" />
              </label>
              <label v-if="serviceName !== 'hub'" class="field">
                <span>Browser</span>
                <select v-model="serviceConfigs[serviceName].browser">
                  <option value="chromium">chromium</option>
                  <option value="firefox">firefox</option>
                  <option value="webkit">webkit</option>
                  <option value="chrome">chrome</option>
                  <option value="edge">edge</option>
                </select>
              </label>
              <label v-if="serviceName !== 'hub'" class="field toggle-field">
                <span>Headless</span>
                <input v-model="serviceConfigs[serviceName].headless" type="checkbox" />
              </label>
            </div>

            <label class="field span-field">
              <span>{{ serviceName === 'hub' ? 'Hub API key' : 'API key' }}</span>
              <input
                v-model="serviceConfigs[serviceName].apiKey"
                type="password"
                :placeholder="serviceName === 'hub' ? 'optional hub bearer key' : 'optional bearer key'"
              />
            </label>

            <div class="action-row">
              <button
                class="primary-button"
                :disabled="store.isBusy(`start:${serviceName}`)"
                @click="store.startService(serviceName)"
              >
                {{ store.isBusy(`start:${serviceName}`) ? 'Launching...' : 'Launch service' }}
              </button>
              <button
                class="secondary-button"
                :disabled="store.isBusy(`stop:${serviceName}`)"
                @click="store.stopService(serviceName)"
              >
                {{ store.isBusy(`stop:${serviceName}`) ? 'Stopping...' : 'Stop' }}
              </button>
              <template v-if="serviceName === 'qwen' || serviceName === 'deepseek' || serviceName === 'kimi'">
                <button
                  class="ghost-button"
                  :disabled="store.isBusy(`provider-login:start:${serviceName}`)"
                  @click="store.startProviderLogin(serviceName)"
                >
                  {{
                    providerLoginOpen(serviceName)
                      ? serviceName === 'qwen'
                        ? 'Global login open'
                        : 'Login open'
                      : serviceName === 'qwen'
                        ? 'Open global login'
                        : 'Open login'
                  }}
                </button>
                <button
                  class="ghost-button"
                  :disabled="!providerLoginOpen(serviceName)"
                  @click="store.stopProviderLogin(serviceName)"
                >
                  {{ serviceName === 'qwen' ? 'Close global login' : 'Close login' }}
                </button>
              </template>
              <template v-if="serviceName === 'hub'">
                <button class="ghost-button" :disabled="!serviceMap.hub?.endpoints.openapi_url" @click="copyText(serviceMap.hub?.endpoints.openapi_url)">
                  Copy OpenAPI
                </button>
                <button class="ghost-button" :disabled="!serviceMap.hub?.endpoints.base_url" @click="copyText(serviceMap.hub?.endpoints.base_url)">
                  Copy base URL
                </button>
              </template>
            </div>

            <dl class="facts">
              <div>
                <dt>PID</dt>
                <dd>{{ serviceMap[serviceName]?.pid ?? 'n/a' }}</dd>
              </div>
              <div>
                <dt>Models</dt>
                <dd>{{ serviceMap[serviceName]?.model_count ?? 0 }}</dd>
              </div>
              <div>
                <dt>Started</dt>
                <dd>{{ formatStarted(serviceMap[serviceName]?.started_at ?? null) }}</dd>
              </div>
            </dl>

            <div class="endpoint-grid">
              <div v-for="(value, key) in serviceMap[serviceName]?.endpoints ?? {}" :key="key" v-show="value" class="info-card">
                <p class="info-label">{{ key.replaceAll('_', ' ') }}</p>
                <p class="mono-line">{{ value }}</p>
              </div>
            </div>

            <div class="model-cloud">
              <span v-for="model in previewModels(serviceMap[serviceName])" :key="`${serviceName}:${model.id}`" class="model-chip">
                {{ model.id }}
              </span>
              <span v-if="!previewModels(serviceMap[serviceName]).length" class="empty-chip">No models loaded yet.</span>
            </div>

            <div class="info-card">
              <p class="info-label">Launch line</p>
              <p class="mono-line">{{ serviceMap[serviceName]?.launch_preview ?? 'not running' }}</p>
            </div>

            <pre class="log-window">{{ serviceMap[serviceName]?.logs?.length ? serviceMap[serviceName]?.logs.join('\n') : 'No recent output.' }}</pre>
          </article>
        </div>

        <aside class="lane side-lane">
          <section class="panel integration-panel">
            <div class="panel-top">
              <div>
                <p class="eyebrow">Unified API</p>
                <h2>Hub integration point</h2>
              </div>
              <span class="status-chip" :data-state="statusTone(serviceMap.hub)">
                {{ hubService?.running ? 'live' : 'offline' }}
              </span>
            </div>

            <div class="info-card">
              <p class="info-label">Base URL</p>
              <p class="mono-line">{{ hubService?.endpoints.base_url ?? 'launch hub to expose the unified API' }}</p>
            </div>

            <div class="info-card">
              <p class="info-label">OpenAPI</p>
              <p class="mono-line">{{ hubService?.endpoints.openapi_url ?? 'launch hub to expose /openapi.json' }}</p>
            </div>

            <div class="provider-grid">
              <article v-for="status in hubProviderStatus" :key="status.provider" class="provider-card">
                <p>{{ status.provider }}</p>
                <strong>{{ status.healthy ? 'reachable' : 'down' }}</strong>
                <span>{{ status.base_url }}</span>
              </article>
            </div>

            <div class="info-card">
              <p class="info-label">Curl starter</p>
              <pre class="code-window">{{ sampleCurl }}</pre>
            </div>
          </section>

          <section class="panel workbench-panel">
            <div class="panel-top">
              <div>
                <p class="eyebrow">Prompt workbench</p>
                <h2>Real request check</h2>
              </div>
              <span class="status-chip" data-state="accent">{{ workbenchService }}</span>
            </div>

            <div class="workbench-grid">
              <label class="field">
                <span>Target</span>
                <select :model-value="workbenchService" @update:model-value="store.setWorkbenchService($event as ServiceName)">
                  <option v-for="serviceName in serviceOrder" :key="serviceName" :value="serviceName">
                    {{ serviceName }}
                  </option>
                </select>
              </label>

              <label class="field span-field">
                <span>Model</span>
                <input v-model="workbenchModel" list="model-options" placeholder="pick or type a model id" />
                <datalist id="model-options">
                  <option v-for="model in availableWorkbenchModels" :key="model" :value="model" />
                </datalist>
              </label>

              <label class="field span-field">
                <span>Prompt</span>
                <textarea v-model="workbenchPrompt" rows="6" placeholder="Enter a real operator prompt for a smoke request." />
              </label>

              <button class="primary-button" :disabled="store.isBusy('workbench:run')" @click="store.runWorkbench()">
                {{ store.isBusy('workbench:run') ? 'Running...' : 'Run request' }}
              </button>
            </div>

            <pre class="code-window large">{{ workbenchResponse || 'The response payload will land here after a live request.' }}</pre>
          </section>

          <section class="panel login-panel">
            <div class="panel-top">
              <div>
                <p class="eyebrow">Login studio</p>
                <h2>Browser auth flows</h2>
                <p class="panel-copy">
                  Playwright works only after real browser login. These controls mirror the old proxy login scripts.
                </p>
              </div>
              <span class="status-chip" data-state="accent">
                {{ (dashboard?.provider_login_sessions.length ?? 0) + (dashboard?.open_login_sessions.length ?? 0) }} active
              </span>
            </div>

            <div class="login-grid">
              <article v-for="provider in providerOrder" :key="provider" class="login-card">
                <div class="panel-top">
                  <div>
                    <p class="eyebrow">{{ provider }}</p>
                    <h3>{{ loginGuides[provider].title }}</h3>
                    <p class="panel-copy">{{ loginGuides[provider].summary }}</p>
                  </div>
                  <span class="status-chip" :data-state="loginTone(provider)">
                    {{ providerLoginOpen(provider) ? 'browser open' : 'ready for login' }}
                  </span>
                </div>

                <div class="action-row">
                  <button
                    class="ghost-button"
                    :disabled="store.isBusy(`provider-login:start:${provider}`)"
                    @click="store.startProviderLogin(provider)"
                  >
                    {{ provider === 'qwen' ? 'Open global login' : 'Open login' }}
                  </button>
                  <button
                    class="secondary-button"
                    :disabled="!providerLoginOpen(provider)"
                    @click="store.stopProviderLogin(provider)"
                  >
                    {{ provider === 'qwen' ? 'Close global login' : 'Close login' }}
                  </button>
                </div>

                <div class="login-steps">
                  <p v-for="step in loginGuides[provider].steps" :key="step" class="step-line">{{ step }}</p>
                </div>

                <div class="info-card">
                  <p class="info-label">Operator note</p>
                  <p class="mono-line">{{ loginGuides[provider].note }}</p>
                </div>
              </article>
            </div>
          </section>

          <section class="panel account-panel">
            <div class="panel-top">
              <div>
                <p class="eyebrow">Qwen accounts</p>
                <h2>Rotation bank</h2>
              </div>
              <span class="status-chip" data-state="accent">{{ dashboard?.qwen_accounts.length ?? 0 }} stored</span>
            </div>

            <div class="info-card">
              <p class="info-label">Flow</p>
              <p class="mono-line">Save email first. Password optional. Then open visible browser login so Playwright can persist profile cookies.</p>
            </div>

            <form class="account-form" @submit.prevent="store.addQwenAccount()">
              <label class="field">
                <span>Email</span>
                <input v-model="qwenEmail" type="email" placeholder="operator@domain.com" />
              </label>
              <label class="field">
                <span>Password</span>
                <input v-model="qwenPassword" type="password" placeholder="optional for auto-login" />
              </label>
              <div class="action-row">
                <button class="primary-button" type="submit" :disabled="store.isBusy('account:add')">
                  {{ store.isBusy('account:add') ? 'Saving...' : 'Save account' }}
                </button>
                <button
                  class="ghost-button"
                  type="button"
                  :disabled="store.isBusy('account:add-open-login')"
                  @click="store.addQwenAccountAndOpenLogin()"
                >
                  {{ store.isBusy('account:add-open-login') ? 'Opening...' : 'Save and open login' }}
                </button>
                <button
                  class="secondary-button"
                  type="button"
                  :disabled="store.isBusy('provider-login:start:qwen')"
                  @click="store.startProviderLogin('qwen')"
                >
                  Open global login
                </button>
              </div>
            </form>

            <div class="account-list">
              <article v-for="account in dashboard?.qwen_accounts ?? []" :key="account.id" class="account-card">
                <div>
                  <p class="account-email">{{ account.email }}</p>
                  <p class="account-meta">{{ account.id }} - {{ account.has_password ? 'password saved' : 'manual login only' }}</p>
                </div>
                <div class="mini-actions">
                  <button
                    class="secondary-button"
                    :disabled="store.isBusy(`login:start:${account.id}`)"
                    @click="store.startQwenLogin(account.id)"
                  >
                    {{ qwenAccountLoginOpen(account.id) ? 'Login open' : 'Open login' }}
                  </button>
                  <button
                    class="ghost-button"
                    :disabled="!qwenAccountLoginOpen(account.id)"
                    @click="store.stopQwenLogin(account.id)"
                  >
                    Close
                  </button>
                  <button
                    class="danger-button"
                    :disabled="store.isBusy(`account:remove:${account.id}`)"
                    @click="store.removeQwenAccount(account.id)"
                  >
                    Remove
                  </button>
                </div>
              </article>
              <p v-if="!(dashboard?.qwen_accounts.length)" class="empty-copy">
                No accounts stored yet. Use global Qwen login for one default profile, or save one account and open its browser login.
              </p>
            </div>
          </section>

          <section class="panel watch-panel">
            <div class="panel-top">
              <div>
                <p class="eyebrow">Stage 2 telemetry</p>
                <h2>Qwen watchdog</h2>
              </div>
              <span class="status-chip" :data-state="qwenService?.admin_status?.watchdog?.overall ?? 'idle'">
                {{ qwenService?.admin_status?.watchdog?.overall ?? 'offline' }}
              </span>
            </div>

            <div class="radar-grid">
              <div class="radar-card">
                <span>Streams</span>
                <strong>{{ qwenService?.admin_status?.watchdog?.active_streams ?? 0 }}</strong>
              </div>
              <div class="radar-card">
                <span>Memory %</span>
                <strong>{{ Math.round(Number(qwenService?.admin_status?.watchdog?.memory_percent ?? 0)) }}</strong>
              </div>
              <div class="radar-card">
                <span>Cooldowns</span>
                <strong>{{ qwenCooldowns.length }}</strong>
              </div>
            </div>

            <div class="cooldown-list">
              <article v-for="cooldown in qwenCooldowns" :key="cooldown.id" class="cooldown-card">
                <p>{{ cooldown.id }}</p>
                <strong>{{ cooldown.remaining }}s</strong>
                <span>{{ cooldown.reason }}</span>
              </article>
              <p v-if="!qwenCooldowns.length" class="empty-copy">
                No active cooldowns. Rotation lane is clear.
              </p>
            </div>
          </section>

          <section class="panel workspace-panel">
            <p class="eyebrow">Workspace</p>
            <h2>Path anchors</h2>
            <p class="mono-block">{{ dashboard?.tools_root ?? 'loading...' }}</p>
            <p class="mono-block">{{ dashboard?.rust_proxy_hub ?? 'loading...' }}</p>
            <p class="footer-line">Version {{ store.version }}</p>
            <pre class="compact-json">{{ prettyJson(serviceMap.hub?.health ?? {}) }}</pre>
          </section>
        </aside>
      </section>
    </main>
  </div>
</template>
