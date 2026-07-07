<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  ChartLineData01Icon,
  CodeCircleIcon,
  CommandLineIcon,
  CpuIcon,
  Database02Icon,
  DashboardSpeed02Icon,
  PlayIcon,
  PulseIcon,
} from '@hugeicons/core-free-icons'
import { computed, nextTick, ref, watch } from 'vue'
import SparkChart from '@/components/dashboard/SparkChart.vue'
import { burstFromElement, playTone } from '@/effects'
import { messageAt } from '@/lib/ui-i18n'
import { useStore } from '@/store'

const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

type WorkbenchTab = 'terminal' | 'chart' | 'json'

const activeTab = ref<WorkbenchTab>('terminal')
const command = ref('')
const terminalEl = ref<HTMLElement | null>(null)
const lines = ref<string[]>([t('workbench.terminalReady'), t('workbench.helpHint')])

const providers = computed(() => store.filteredProviders)
const liveProviders = computed(() => providers.value.filter((provider) => provider.running))
const hubOnline = computed(() => store.overview?.hub.running ?? false)
const modelOptions = computed(() => store.hubModelOptions)
const selectedProvider = computed(() => store.workbenchModel.split(':')[0] || t('workbench.noModel'))
const totalModels = computed(() => providers.value.reduce((sum, provider) => sum + provider.model_count, 0))
const healthyProviders = computed(() => providers.value.filter((provider) => provider.health_status === 'healthy').length)
const avgLatency = computed(() => {
  if (!liveProviders.value.length) return 0
  return Math.round(liveProviders.value.reduce((sum, provider) => sum + (provider.latency_ms ?? 0), 0) / liveProviders.value.length)
})
const throughputSeries = computed(() => store.overview?.throughput_series ?? [])
const latencySeries = computed(() => store.overview?.latency_series ?? [])
const responseJson = computed(() => store.workbenchResponse || t('workbench.emptyJson'))

const tabs = computed<Array<{ key: WorkbenchTab; label: string; icon: unknown; hint: string }>>(() => [
  { key: 'terminal', label: t('workbench.tabs.terminal'), icon: CommandLineIcon, hint: t('workbench.tabHints.terminal') },
  { key: 'chart', label: t('workbench.tabs.chart'), icon: ChartLineData01Icon, hint: t('workbench.tabHints.chart') },
  { key: 'json', label: t('workbench.tabs.json'), icon: CodeCircleIcon, hint: t('workbench.tabHints.json') },
])

const healthCards = computed(() => [
  { key: 'providers', label: t('workbench.providersVisible'), value: String(providers.value.length), icon: PulseIcon },
  { key: 'models', label: t('workbench.totalModels'), value: String(totalModels.value), icon: CpuIcon },
  { key: 'healthy', label: t('workbench.healthy'), value: String(healthyProviders.value), icon: Database02Icon },
  { key: 'latency', label: 'avg latency', value: `${avgLatency.value}ms`, icon: DashboardSpeed02Icon },
])

function scrollTerminal() {
  void nextTick(() => {
    if (terminalEl.value) terminalEl.value.scrollTop = terminalEl.value.scrollHeight
  })
}

function write(line: string | string[]) {
  if (Array.isArray(line)) lines.value.push(...line)
  else lines.value.push(line)
  if (lines.value.length > 90) lines.value = lines.value.slice(-90)
  scrollTerminal()
}

function helpLines() {
  const menu = messageAt(store.locale, 'workbench.helpMenu')
  return Array.isArray(menu) ? menu.map(String) : [t('workbench.helpHint')]
}

function runStatusCommand() {
  write([
    `stdout: hub=${hubOnline.value ? 'online' : 'offline'} runtime=${store.runtimeReady ? 'ready' : 'blocked'} provider=${selectedProvider.value}`,
    `stdout: visible=${providers.value.length} live=${liveProviders.value.length} models=${totalModels.value} open_logins=${store.openLoginCount}`,
  ])
  if (store.runtimeIssues.length) write(store.runtimeIssues.map((issue) => `stderr: ${issue}`))
}

function runProvidersCommand() {
  if (!providers.value.length) {
    write(t('workbench.noProvidersVisible'))
    return
  }
  write(
    providers.value.map(
      (provider) =>
        `stdout: ${provider.name.padEnd(8)} ${provider.running ? 'running' : 'stopped'} ${provider.health_status} models=${provider.model_count} latency=${provider.latency_ms ?? 0}ms`,
    ),
  )
}

function runModelsCommand() {
  if (!modelOptions.value.length) {
    write(t('workbench.noModelsYet'))
    return
  }
  write(modelOptions.value.map((model) => `stdout: ${model}`))
}

function setActiveTab(tab: WorkbenchTab) {
  activeTab.value = tab
  if (tab === 'chart') write(t('workbench.chartOpened'))
  if (tab === 'json') write(t('workbench.jsonOpened'))
}

async function runProbe(event?: MouseEvent) {
  if (event?.currentTarget instanceof Element) burstFromElement(event.currentTarget, '#ff9500', 18)
  if (!store.runtimeReady) {
    write(t('workbench.runtimeBlocked'))
    playTone('error')
    return
  }
  if (store.openLoginCount > 0) {
    write(t('workbench.closeLoginFirst'))
    playTone('error')
    return
  }
  write(t('workbench.running'))
  playTone('boot')
  await store.runWorkbench()
  if (store.error) write(`stderr: ${store.error}`)
  else {
    write(t('workbench.finished'))
    activeTab.value = 'json'
    playTone('success')
  }
}

async function executeCommand() {
  const raw = command.value.trim()
  if (!raw) return
  write(`operator@rustproxyhub:~$ ${raw}`)
  command.value = ''
  const [name = '', ...args] = raw.split(/\s+/)
  const rest = args.join(' ')

  switch (name.toLowerCase()) {
    case 'help':
      write(helpLines())
      break
    case 'clear':
      lines.value = []
      break
    case 'status':
      runStatusCommand()
      break
    case 'providers':
      runProvidersCommand()
      break
    case 'models':
      runModelsCommand()
      break
    case 'prompt':
      if (rest) store.workbenchPrompt = rest
      write(t('workbench.promptUpdated'))
      break
    case 'search':
      store.workbenchWebSearch = rest.toLowerCase() === 'on' || rest.toLowerCase() === 'true'
      write(store.workbenchWebSearch ? t('workbench.webSearchOn') : t('workbench.webSearchOff'))
      break
    case 'run':
      await runProbe()
      break
    case 'charts':
    case 'chart':
      setActiveTab('chart')
      break
    case 'json':
      setActiveTab('json')
      break
    default:
      write(t('workbench.notFound', { command: name }))
      playTone('error')
  }
}

watch(
  () => store.locale,
  () => {
    if (lines.value.length <= 2) lines.value = [t('workbench.terminalReady'), t('workbench.helpHint')]
  },
)

watch(modelOptions, () => store.syncWorkbenchModel(), { immediate: true })
</script>

<template>
  <div class="workbench-panel">
    <header class="panel-head">
      <div>
        <span class="eyebrow"><HugeiconsIcon :icon="CommandLineIcon" :size="12" aria-hidden="true" /> {{ t('common.workbench') }}</span>
        <h2 class="panel-title">{{ t('app.headerTabs.workbench') }}</h2>
      </div>
      <div class="workbench-status" :class="{ ready: store.runtimeReady && hubOnline, blocked: !store.runtimeReady }">
        <span class="state-dot" />
        <span>{{ store.runtimeReady ? (hubOnline ? t('app.stackOperational') : t('app.statusHubPending')) : t('app.stackDiagnosing') }}</span>
      </div>
    </header>

    <div v-if="!store.runtimeReady && store.runtimeIssues.length" class="panel-alert">
      {{ store.runtimeIssues[0] }}
    </div>

    <div class="workbench-grid">
      <section class="card control-card">
        <div class="control-row">
          <label class="field-block">
            <span>{{ t('workbench.model') }}</span>
            <input
              v-model="store.workbenchModel"
              list="hub-model-options"
              class="control-input"
              :disabled="!store.runtimeReady"
              :placeholder="t('workbench.noModelSelected')"
            />
            <datalist id="hub-model-options">
              <option v-for="model in modelOptions" :key="model" :value="model" />
            </datalist>
          </label>
          <label class="toggle-block">
            <input v-model="store.workbenchWebSearch" type="checkbox" :disabled="!store.runtimeReady" />
            <span>{{ t('workbench.webSearch') }}</span>
          </label>
        </div>

        <label class="field-block prompt-block">
          <span>{{ t('workbench.prompt') }}</span>
          <textarea
            v-model="store.workbenchPrompt"
            class="control-input prompt-input"
            rows="4"
            :placeholder="t('workbench.promptPlaceholder')"
            :disabled="!store.runtimeReady"
          />
        </label>

        <div class="control-actions">
          <button type="button" class="cmd-chip" @click="runStatusCommand">{{ t('workbench.buttons.status') }}</button>
          <button type="button" class="cmd-chip" @click="runModelsCommand">{{ t('workbench.buttons.models') }}</button>
          <button type="button" class="cmd-chip" @click="lines = []">{{ t('workbench.buttons.clear') }}</button>
          <button
            type="button"
            class="run-button"
            :disabled="!store.runtimeReady || store.isBusy('workbench:run')"
            @click="runProbe($event)"
          >
            <HugeiconsIcon :icon="PlayIcon" :size="14" aria-hidden="true" />
            {{ store.isBusy('workbench:run') ? t('workbench.buttons.running') : t('workbench.buttons.run') }}
          </button>
        </div>
      </section>

      <section class="workbench-main">
        <div class="workbench-tabs" role="tablist" aria-label="Workbench views">
          <button
            v-for="tab in tabs"
            :id="`workbench-tab-${tab.key}`"
            :key="tab.key"
            type="button"
            role="tab"
            :aria-selected="activeTab === tab.key"
            :aria-controls="`workbench-panel-${tab.key}`"
            class="workbench-tab"
            :class="{ active: activeTab === tab.key }"
            @click="setActiveTab(tab.key)"
          >
            <HugeiconsIcon :icon="tab.icon" :size="14" aria-hidden="true" />
            <span>{{ tab.label }}</span>
            <small>{{ tab.hint }}</small>
          </button>
        </div>

        <div
          v-show="activeTab === 'terminal'"
          id="workbench-panel-terminal"
          role="tabpanel"
          aria-labelledby="workbench-tab-terminal"
          class="card terminal-card"
        >
          <div ref="terminalEl" class="terminal-output">
            <p v-for="(line, index) in lines" :key="`${index}-${line}`" :class="{ error: line.startsWith('stderr'), ok: line.startsWith('stdout') }">
              {{ line }}
            </p>
          </div>
          <form class="terminal-input-row" @submit.prevent="executeCommand">
            <span>$</span>
            <input v-model="command" class="terminal-input" autocomplete="off" spellcheck="false" placeholder="help" />
          </form>
        </div>

        <div
          v-show="activeTab === 'chart'"
          id="workbench-panel-chart"
          role="tabpanel"
          aria-labelledby="workbench-tab-chart"
          class="card chart-lab-card"
        >
          <div class="health-grid">
            <article v-for="card in healthCards" :key="card.key" class="health-card">
              <HugeiconsIcon :icon="card.icon" :size="16" aria-hidden="true" />
              <span>{{ card.label }}</span>
              <strong>{{ card.value }}</strong>
            </article>
          </div>

          <div class="chart-stack">
            <div class="chart-panel">
              <div class="chart-title">{{ t('workbench.providerMetrics') }}</div>
              <SparkChart :values="throughputSeries" :height="86" mode="bars" stroke="rgba(255,149,0,0.7)" />
            </div>
            <div class="chart-panel">
              <div class="chart-title">{{ t('workbench.hubStream') }}</div>
              <SparkChart :values="latencySeries" :height="86" stroke="#5ce1e6" fill="rgba(92,225,230,0.16)" />
            </div>
          </div>

          <div class="provider-table">
            <div v-for="provider in providers" :key="provider.name" class="provider-row">
              <span class="provider-dot" :class="{ on: provider.running, warn: provider.health_status === 'degraded' }" />
              <strong>{{ provider.name }}</strong>
              <span>{{ provider.model_count }} models</span>
              <span>{{ provider.running ? `${provider.latency_ms ?? 0}ms` : '--' }}</span>
              <span>{{ provider.running ? `${(provider.error_rate ?? 0).toFixed(1)}% err` : 'offline' }}</span>
            </div>
          </div>
        </div>

        <div
          v-show="activeTab === 'json'"
          id="workbench-panel-json"
          role="tabpanel"
          aria-labelledby="workbench-tab-json"
          class="card json-card"
        >
          <div class="json-head">
            <span>response.json</span>
            <span>{{ selectedProvider }}</span>
          </div>
          <pre>{{ responseJson }}</pre>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.workbench-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.2rem;
}
.panel-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
.eyebrow {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-family: var(--font-arcade);
  font-size: 0.5rem;
  color: var(--orange);
  letter-spacing: 0.1em;
  text-transform: uppercase;
}
.panel-title {
  font-family: var(--font-arcade);
  font-size: 0.88rem;
  margin-top: 0.5rem;
  color: var(--text);
}
.workbench-status {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  border: 1px solid var(--line);
  background: var(--void-2);
  border-radius: var(--radius-sm);
  color: var(--muted-strong);
  padding: 0.45rem 0.7rem;
  font-size: 0.62rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.state-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--orange);
  box-shadow: var(--glow-orange);
}
.workbench-status.ready {
  color: var(--cyan);
  border-color: rgba(92, 225, 230, 0.38);
}
.workbench-status.ready .state-dot { background: var(--cyan); box-shadow: var(--glow-cyan); }
.workbench-status.blocked { color: var(--danger); border-color: rgba(255, 91, 110, 0.35); }
.workbench-status.blocked .state-dot { background: var(--danger); box-shadow: 0 0 12px rgba(255, 91, 110, 0.35); }

.workbench-grid {
  display: grid;
  grid-template-columns: minmax(16rem, 0.82fr) minmax(0, 1.35fr);
  gap: 0.9rem;
}
.control-card {
  padding: 0.95rem;
  display: flex;
  flex-direction: column;
  gap: 0.8rem;
}
.control-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 0.65rem;
  align-items: end;
}
.field-block {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  min-width: 0;
}
.field-block > span,
.toggle-block span {
  font-size: 0.57rem;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}
.control-input,
.terminal-input {
  width: 100%;
  border: 1px solid var(--line);
  background: var(--void-2);
  color: var(--text);
  border-radius: var(--radius-sm);
  padding: 0.58rem 0.62rem;
  font-family: var(--font-mono);
  font-size: 0.7rem;
}
.control-input:focus,
.terminal-input:focus {
  outline: none;
  border-color: var(--orange);
  box-shadow: inset 0 0 0 1px var(--orange-soft);
}
/* restore outline for keyboard focus — :focus above killed it */
.control-input:focus-visible,
.terminal-input:focus-visible {
  outline: 2px solid var(--orange);
  outline-offset: 2px;
}
.prompt-input {
  min-height: 7.5rem;
  resize: vertical;
  line-height: 1.45;
}
.toggle-block {
  min-height: 2.2rem;
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.58rem 0.65rem;
  white-space: nowrap;
}
.toggle-block input { accent-color: var(--orange); }
.control-actions {
  display: flex;
  gap: 0.42rem;
  flex-wrap: wrap;
  margin-top: auto;
}
.cmd-chip,
.run-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 0.35rem;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted-strong);
  border-radius: var(--radius-sm);
  padding: 0.5rem 0.65rem;
  cursor: pointer;
  font-size: 0.58rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  transition: all 0.15s ease;
}
.cmd-chip:hover,
.run-button:hover:not(:disabled) {
  color: var(--orange);
  border-color: var(--orange);
  background: var(--orange-soft);
}
.run-button {
  flex: 1;
  min-width: 9rem;
  color: var(--void);
  border-color: var(--orange);
  background: var(--orange);
  font-weight: 800;
}
.run-button:disabled { opacity: 0.55; cursor: not-allowed; }

.workbench-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
}
.workbench-tabs {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 0.45rem;
}
.workbench-tab {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 0.25rem;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted-strong);
  border-radius: var(--radius-sm);
  padding: 0.58rem 0.65rem;
  cursor: pointer;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.workbench-tab span {
  font-family: var(--font-arcade);
  font-size: 0.54rem;
  color: inherit;
}
.workbench-tab small {
  color: var(--muted);
  font-size: 0.52rem;
  text-transform: none;
  letter-spacing: 0;
}
.workbench-tab.active {
  color: var(--orange);
  border-color: var(--orange);
  background: var(--orange-soft);
}
.terminal-card,
.json-card,
.chart-lab-card {
  min-height: 22.5rem;
}
.terminal-card {
  display: flex;
  flex-direction: column;
}
.terminal-output {
  height: 19rem;
  overflow: auto;
  padding: 0.85rem;
  background:
    linear-gradient(rgba(255, 149, 0, 0.035) 1px, transparent 1px),
    var(--void-2);
  background-size: 100% 22px;
}
.terminal-output p {
  margin: 0 0 0.22rem;
  color: var(--muted-strong);
  font-size: 0.68rem;
  line-height: 1.45;
  white-space: pre-wrap;
  word-break: break-word;
}
.terminal-output p.ok { color: var(--cyan); }
.terminal-output p.error { color: #ffc1c8; }
.terminal-input-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  border-top: 1px solid var(--line);
  padding: 0.55rem 0.7rem;
  color: var(--orange);
}
.terminal-input {
  border: none;
  background: transparent;
  padding: 0.25rem 0;
}
.terminal-input:focus { box-shadow: none; }

.chart-lab-card,
.json-card {
  padding: 0.9rem;
}
.health-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.45rem;
  margin-bottom: 0.8rem;
}
.health-card {
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: rgba(0, 0, 0, 0.2);
  padding: 0.6rem;
  display: flex;
  flex-direction: column;
  gap: 0.28rem;
  color: var(--muted-strong);
}
.health-card svg { color: var(--orange); }
.health-card span {
  font-size: 0.52rem;
  text-transform: uppercase;
  color: var(--muted);
}
.health-card strong {
  font-family: var(--font-arcade);
  font-size: 0.72rem;
  color: var(--text);
}
.chart-stack {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.6rem;
}
.chart-panel {
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--void-2);
  padding: 0.65rem;
}
.chart-title,
.json-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--muted);
  font-size: 0.56rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  margin-bottom: 0.55rem;
}
.provider-table {
  margin-top: 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}
.provider-row {
  display: grid;
  grid-template-columns: auto minmax(5rem, 1fr) repeat(3, auto);
  gap: 0.55rem;
  align-items: center;
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
  padding: 0.42rem 0.55rem;
  color: var(--muted-strong);
  font-size: 0.62rem;
}
.provider-row strong {
  color: var(--text);
  text-transform: uppercase;
}
.provider-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--muted);
}
.provider-dot.on { background: var(--cyan); box-shadow: var(--glow-cyan); }
.provider-dot.warn { background: var(--orange); box-shadow: var(--glow-orange); }
.json-card pre {
  margin: 0;
  height: 20rem;
  overflow: auto;
  color: var(--cyan);
  background: var(--void-2);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.8rem;
  font-size: 0.68rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}

@media (max-width: 920px) {
  .workbench-grid,
  .chart-stack {
    grid-template-columns: 1fr;
  }
  .health-grid {
    grid-template-columns: repeat(2, 1fr);
  }
}
@media (max-width: 560px) {
  .control-row,
  .workbench-tabs,
  .health-grid,
  .provider-row {
    grid-template-columns: 1fr;
  }
}
</style>
