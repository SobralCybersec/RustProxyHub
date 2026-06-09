<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Chart from 'chart.js/auto'
import { useStore } from '@/store'

const store = useStore()
const { hubModelOptions, overview } = storeToRefs(store)
type Provider = {
  name?: string
  model_count?: number
  models?: unknown[]
  health_status?: string
  login_state?: string
  base_url?: string
}
const runtimeBlocked = computed(() => (overview.value ? !overview.value.runtime.single_runner_ready : false))
const loginSessionOpen = computed(() => store.openLoginCount > 0)
const workbenchBlocked = computed(() => runtimeBlocked.value || loginSessionOpen.value)
const runtimeIssueText = computed(() => overview.value?.runtime.issues.join(' ') ?? '')
const providers = computed<Provider[]>(() => (store.filteredProviders ?? []) as Provider[])

const workbenchModel = computed({
  get: () => store.workbenchModel,
  set: (val) => { store.workbenchModel = val },
})

type ConsoleTab = 'terminal' | 'chart' | 'json'
const activeTab = ref<ConsoleTab>('terminal')
const terminalInput = ref('')
const terminalLines = ref<string[]>([
  'RustProxy probe console initialized.',
  "Type 'help' to list probe commands.",
])
const commandHistory = ref<string[]>([])
const historyIndex = ref(-1)
const terminalInputRef = ref<HTMLInputElement | null>(null)
const terminalScrollRef = ref<HTMLElement | null>(null)
const chartCanvas = ref<HTMLCanvasElement | null>(null)
let providerChart: Chart | null = null

const tabs: Array<{ key: ConsoleTab; label: string; hint: string }> = [
  { key: 'terminal', label: 'terminal', hint: 'interactive shell' },
  { key: 'chart', label: 'chartjs', hint: 'provider metrics' },
  { key: 'json', label: 'response.json', hint: 'hub output' },
]

const terminalPrefix = computed(() => {
  const model = store.workbenchModel || 'no-model'
  return `visitor@rustproxy:${model}$`
})

const chartLabels = computed(() => providers.value.map((provider: Provider) => provider.name ?? ''))
const chartModels = computed(() => providers.value.map((provider: Provider) => Number(provider.model_count ?? provider.models?.length ?? 0)))
const chartHealth = computed(() => providers.value.map((provider: Provider) => provider.health_status === 'ok' ? 1 : 0))
const totalModels = computed(() => chartModels.value.reduce((sum: number, count: number) => sum + count, 0))
const healthyProviders = computed(() => chartHealth.value.reduce((sum: number, count: number) => sum + count, 0))

function scrollTerminal() {
  void nextTick(() => terminalScrollRef.value?.scrollTo({ top: terminalScrollRef.value.scrollHeight }))
}

function writeTerminal(lines: string | string[]) {
  terminalLines.value.push(...(Array.isArray(lines) ? lines : [lines]))
  scrollTerminal()
}

function focusTerminal() {
  activeTab.value = 'terminal'
  void nextTick(() => terminalInputRef.value?.focus())
}

function providerStatusLines() {
  if (!providers.value.length) return ['stdout: no providers visible with the current filter.']
  return providers.value.map((provider: Provider) => {
    const status = provider.login_state?.replaceAll('_', ' ') ?? provider.health_status ?? 'unknown'
    const models = provider.model_count ?? provider.models?.length ?? 0
    return `${String(provider.name ?? '').padEnd(10)} | ${String(status).padEnd(20)} | ${String(models).padStart(3)} models | ${provider.base_url ?? 'no base url'}`
  })
}

async function executeCommand(rawCommand = terminalInput.value) {
  const command = rawCommand.trim()
  if (!command) return

  writeTerminal(`${terminalPrefix.value} ${command}`)
  commandHistory.value.unshift(command)
  historyIndex.value = -1
  terminalInput.value = ''

  if (command === 'clear') {
    terminalLines.value = []
    return
  }

  if (command === 'help') {
    writeTerminal([
      'Available commands:',
      '  help              show this menu',
      '  clear             clear terminal output',
      '  status            print hub/runtime status',
      '  providers         list visible provider dossiers',
      '  models            list hub model options',
      '  prompt <text>     replace the live probe prompt',
      '  search on|off     toggle normalized web-search flag',
      '  run               execute the live hub probe',
      '  charts            open Chart.js metrics',
      '  json              open the raw response pane',
    ])
    return
  }

  if (command === 'status') {
    writeTerminal([
      `hub.running        ${overview.value?.hub.running ? 'true' : 'false'}`,
      `hub.health         ${overview.value?.hub.health_status ?? 'unknown'}`,
      `runtime.ready      ${overview.value?.runtime.single_runner_ready ? 'true' : 'false'}`,
      `open.login.windows ${store.openLoginCount}`,
      `visible.providers  ${providers.value.length}`,
    ])
    return
  }

  if (command === 'providers') {
    writeTerminal(providerStatusLines())
    return
  }

  if (command === 'models') {
    writeTerminal(hubModelOptions.value.length ? hubModelOptions.value.map((model) => `- ${model}`) : 'stdout: no hub model options discovered yet.')
    return
  }

  if (command.startsWith('prompt ')) {
    store.workbenchPrompt = command.slice('prompt '.length).trim()
    writeTerminal('stdout: prompt updated')
    return
  }

  if (command === 'search on' || command === 'search off') {
    store.workbenchWebSearch = command.endsWith('on')
    writeTerminal(`stdout: web search ${store.workbenchWebSearch ? 'enabled' : 'disabled'}`)
    return
  }

  if (command === 'charts') {
    activeTab.value = 'chart'
    writeTerminal('stdout: opened chartjs pane')
    return
  }

  if (command === 'json') {
    activeTab.value = 'json'
    writeTerminal('stdout: opened response.json pane')
    return
  }

  if (command === 'run') {
    if (workbenchBlocked.value) {
      writeTerminal(runtimeBlocked.value ? `stderr: runtime blocked: ${runtimeIssueText.value}` : 'stderr: login browser is open. Close it before running a live probe.')
      return
    }
    writeTerminal('stdout: running live hub probe...')
    try {
      await store.runWorkbench()
      writeTerminal('stdout: probe finished. Open response.json for output.')
    } catch (error) {
      writeTerminal(`stderr: probe failed: ${error instanceof Error ? error.message : String(error)}`)
    }
    return
  }

  writeTerminal(`stderr: command not found: ${command}. Type 'help' for more information.`)
}

function handleTerminalKey(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    event.preventDefault()
    void executeCommand()
    return
  }
  if (event.key === 'ArrowUp') {
    event.preventDefault()
    if (historyIndex.value + 1 >= commandHistory.value.length) return
    historyIndex.value += 1
    terminalInput.value = commandHistory.value[historyIndex.value] ?? ''
    return
  }
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    if (historyIndex.value <= 0) {
      historyIndex.value = -1
      terminalInput.value = ''
      return
    }
    historyIndex.value -= 1
    terminalInput.value = commandHistory.value[historyIndex.value] ?? ''
  }
}

function renderChart() {
  if (!chartCanvas.value) return
  const labels = chartLabels.value.length ? chartLabels.value : ['No providers']
  const models = chartModels.value.length ? chartModels.value : [0]
  const healthy = chartHealth.value.length ? chartHealth.value : [0]

  if (providerChart) {
    providerChart.data.labels = labels
    providerChart.data.datasets[0].data = models
    providerChart.data.datasets[1].data = healthy
    providerChart.update()
    return
  }

  providerChart = new Chart(chartCanvas.value, {
    type: 'bar',
    data: {
      labels,
      datasets: [
        { label: 'Models discovered', data: models, borderWidth: 1 },
        { label: 'Healthy status', data: healthy, borderWidth: 1, type: 'line', tension: 0.25 },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { intersect: false, mode: 'index' },
      plugins: { legend: { labels: { color: 'lightgreen' } } },
      scales: {
        x: { ticks: { color: 'lightgreen' }, grid: { color: 'rgba(144, 238, 144, 0.14)' } },
        y: { beginAtZero: true, ticks: { color: 'lightgreen', precision: 0 }, grid: { color: 'rgba(144, 238, 144, 0.14)' } },
      },
    },
  })
}

watch([chartLabels, chartModels, chartHealth], () => renderChart(), { deep: true })
watch(activeTab, (tab) => {
  if (tab === 'chart') void nextTick(renderChart)
  if (tab === 'terminal') void nextTick(() => terminalInputRef.value?.focus())
})

onMounted(() => {
  renderChart()
  focusTerminal()
})

onBeforeUnmount(() => {
  providerChart?.destroy()
  providerChart = null
})
</script>

<template>
  <section class="panel workbench-panel terminal-workbench">
    <div class="terminal-line"><span class="prompt">visitor@rustproxy:~$</span> node probe.ts --interactive</div>

    <nav class="workbench-tabs" aria-label="Workbench tabs">
      <button v-for="tab in tabs" :key="tab.key" class="workbench-tab" :class="{ active: activeTab === tab.key }" type="button" @click.stop="activeTab = tab.key">
        <strong>{{ tab.label }}</strong>
        <span>{{ tab.hint }}</span>
      </button>
    </nav>

    <div v-if="runtimeBlocked" class="panel-alert"><strong>stderr:</strong> {{ runtimeIssueText }}</div>
    <div v-else-if="loginSessionOpen" class="panel-alert"><strong>stderr:</strong> Close active login session before running a live probe.</div>

    <div v-show="activeTab === 'terminal'" class="tab-panel terminal-tab-panel">
      <div class="terminal-screen" @click.stop="focusTerminal">
        <div class="terminal-bar"><span>RUSTPROXY://WORKBENCH</span><span>{{ store.workbenchModel || 'no model' }}</span></div>
        <div ref="terminalScrollRef" class="terminal-output" aria-live="polite">
          <p v-for="(line, index) in terminalLines" :key="`${line}-${index}`">{{ line }}</p>
        </div>
        <label class="terminal-command-line">
          <span>{{ terminalPrefix }}</span>
          <input ref="terminalInputRef" v-model="terminalInput" autocomplete="off" spellcheck="false" :disabled="store.isBusy('workbench:run')" @keydown="handleTerminalKey" />
        </label>
      </div>

      <div class="workbench-grid">
        <label class="field span-field">
          <span>Model</span>
          <input v-model="workbenchModel" list="hub-model-options" placeholder="qwen:model-id or chatgpt:model-id" :disabled="workbenchBlocked" />
          <datalist id="hub-model-options">
            <option v-for="model in hubModelOptions" :key="model" :value="model" />
          </datalist>
        </label>
        <label class="field toggle-field">
          <span>Web search</span>
          <input v-model="store.workbenchWebSearch" type="checkbox" :disabled="workbenchBlocked" />
        </label>
        <label class="field span-field prompt-field">
          <span>Prompt</span>
          <textarea v-model="store.workbenchPrompt" rows="5" placeholder="Ask for a smoke response and confirm which provider answered." :disabled="workbenchBlocked" />
        </label>
        <div class="action-row">
          <button type="button" @click.stop="executeCommand('status')">status</button>
          <button type="button" @click.stop="executeCommand('models')">models</button>
          <button type="button" @click.stop="executeCommand('clear')">clear</button>
          <button type="button" :disabled="store.isBusy('workbench:run') || workbenchBlocked" @click.stop="executeCommand('run')">
            {{ store.isBusy('workbench:run') ? 'running...' : 'run live probe' }}
          </button>
        </div>
      </div>
    </div>

    <div v-show="activeTab === 'chart'" class="tab-panel chart-panel">
      <div class="chart-summary-grid">
        <article><span>visible providers</span><strong>{{ providers.length }}</strong></article>
        <article><span>total models</span><strong>{{ totalModels }}</strong></article>
        <article><span>healthy</span><strong>{{ healthyProviders }}/{{ providers.length }}</strong></article>
      </div>
      <div class="chart-shell">
        <div class="terminal-bar"><span>PROVIDER METRICS</span><span>Chart.js</span></div>
        <div class="chart-canvas-wrap"><canvas ref="chartCanvas" aria-label="Provider models and health chart" role="img"></canvas></div>
      </div>
    </div>

    <div v-show="activeTab === 'json'" class="tab-panel json-panel">
      <div class="json-shell">
        <div class="terminal-bar"><span>HUB STREAM</span><span>{{ store.workbenchModel || 'no model selected' }}</span></div>
        <pre>{{ store.workbenchResponse || 'The live JSON response lands here.' }}</pre>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* hard square terminal reset */
*,
*::before,
*::after {
  border-radius: 0 !important;
}

:deep(*),
:deep(*::before),
:deep(*::after) {
  border-radius: 0 !important;
}

input, textarea, select { user-select: text; }

.terminal-workbench { width: 100%; padding: .9rem; color: lightgreen; }
.terminal-line { color: #b9ffd0; font-size: 1.2rem; margin-bottom: .75rem; }
.prompt { color: #7fff9a; margin-right: .45rem; }
.workbench-tabs { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .45rem; margin-bottom: .7rem; }
.workbench-tab {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #030903;
  color: lightgreen;
  padding: .5rem .65rem;
  text-align: left;
  cursor: pointer;
  font: inherit;
}
.workbench-tab.active,
.workbench-tab:hover { background: rgba(144, 238, 144, .16); }
.workbench-tab strong,
.workbench-tab span { display: block; }
.workbench-tab span { color: rgba(196, 255, 202, .66); }
.panel-alert { border: 1px solid rgba(255, 82, 82, .55); background: rgba(80,0,0,.2); color: #ffc1c1; padding: .55rem .65rem; margin-bottom: .7rem; }
.terminal-screen,
.chart-shell,
.json-shell { border: 1px solid rgba(144, 238, 144, .42); background: #000; }
.terminal-bar { display: flex; justify-content: space-between; gap: 1rem; padding: .45rem .65rem; border-bottom: 1px solid rgba(144, 238, 144, .35); background: rgba(144, 238, 144, .12); }
.terminal-output { min-height: 12rem; max-height: 19rem; overflow: auto; padding: .65rem; }
.terminal-output p { margin: 0; white-space: pre-wrap; word-break: break-word; }
.terminal-command-line { display: flex; gap: .5rem; align-items: center; border-top: 1px solid rgba(144, 238, 144, .28); padding: .45rem .65rem; }
.terminal-command-line span { flex-shrink: 0; }
.terminal-command-line input { flex: 1; border: 0; background: transparent; color: lightgreen; outline: none; font: inherit; }
.workbench-grid { display: grid; grid-template-columns: 1fr auto; gap: .65rem; margin-top: .7rem; }
.field { display: grid; gap: .35rem; }
.field span { color: #7fff9a; text-transform: uppercase; letter-spacing: .1em; }
.span-field { grid-column: span 1; }
.prompt-field { grid-column: 1 / -1; }
input,
textarea { border: 1px solid rgba(144, 238, 144, .42); background: #000; color: lightgreen; padding: .6rem; font: inherit; outline: none; }
textarea { resize: vertical; }
.action-row { grid-column: 1 / -1; display: flex; flex-wrap: wrap; gap: .4rem; }
button { border: 1px solid rgba(144, 238, 144, .42); background: #041004; color: lightgreen; padding: .5rem .65rem; cursor: pointer; font: inherit; }
button:hover { background: rgba(144, 238, 144, .16); }
button:disabled { opacity: .42; cursor: not-allowed; }
.chart-summary-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: .55rem; margin-bottom: .7rem; }
.chart-summary-grid article { border: 1px solid rgba(144, 238, 144, .35); background: #020702; padding: .65rem; }
.chart-summary-grid span { display: block; color: rgba(196, 255, 202, .72); }
.chart-summary-grid strong { display: block; color: #dfffe4; font-size: 2rem; font-weight: 400; }
.chart-canvas-wrap { height: 24rem; padding: .65rem; }
.json-shell pre { margin: 0; min-height: 22rem; padding: .65rem; overflow: auto; white-space: pre-wrap; word-break: break-word; color: rgba(196, 255, 202, .82); }
@media (max-width: 760px) {
  .workbench-tabs,
  .workbench-grid,
  .chart-summary-grid { grid-template-columns: 1fr; }
  .terminal-command-line { align-items: flex-start; flex-direction: column; }
}
</style>
