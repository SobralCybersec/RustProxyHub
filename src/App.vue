<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  AiBrain03Icon,
  AiChat02Icon,
  AppWindowIcon,
  Cancel01Icon,
  ComputerTerminalIcon,
  DashboardSquare03Icon,
  HelpCircleIcon,
  Key02Icon,
  Maximize01Icon,
  Minimize01Icon,
  ServerStack03Icon,
} from '@hugeicons/core-free-icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { storeToRefs } from 'pinia'
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue'
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'

const store = useStore()
const { overview, error, notice, filteredProviders } = storeToRefs(store)

type ConsoleTab = 'overview' | 'providers' | 'access' | 'qwen' | 'workbench'

const tutorialStorageKey = 'rustproxyhub:tutorial-complete'
const activeTab = ref<ConsoleTab>('overview')
const terminalInput = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const outputRef = ref<HTMLElement | null>(null)
const history = ref<string[]>([])
const historyIndex = ref(-1)
const tutorialOpen = ref(false)
const tutorialSlide = ref(0)
const output = ref<string[]>([
  'RustProxyHub operator console ready.',
  "Type 'help' for commands or use the left rail.",
])

let clockTimer: number | null = null
const now = ref(new Date())

const tabs = computed(() => [
  { key: 'overview', label: 'Overview', meta: overview.value?.hub.health_status ?? 'booting', icon: DashboardSquare03Icon },
  { key: 'providers', label: 'Providers', meta: `${filteredProviders.value.length} visible`, icon: ServerStack03Icon },
  { key: 'access', label: 'Access', meta: `${store.openLoginCount} active`, icon: Key02Icon },
  { key: 'qwen', label: 'Qwen Vault', meta: `${store.qwenAccounts.length} stored`, icon: AiBrain03Icon },
  { key: 'workbench', label: 'Workbench', meta: store.workbenchModel || 'no model', icon: ComputerTerminalIcon },
] as const)

const tutorialSlides = [
  {
    title: 'Runtime Preflight',
    body: 'Check bundled Node, Playwright helper, Edge availability, and writable app data before opening provider sessions.',
  },
  {
    title: 'Provider Sessions',
    body: 'Use Access to open browser-backed logins. Close each login window before running live hub probes.',
  },
  {
    title: 'Model Discovery',
    body: 'Refresh after login. Browser providers report discovered session models and keep a safe fallback when discovery fails.',
  },
]

const prefix = computed(() => `operator@rustproxy:${activeTab.value}$`)
const clock = computed(() => now.value.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }))
const runtimeState = computed(() => (store.runtimeReady ? 'ready' : 'blocked'))

function scrollOutput() {
  void nextTick(() => outputRef.value?.scrollTo({ top: outputRef.value.scrollHeight }))
}

function write(lines: string | string[]) {
  output.value.push(...(Array.isArray(lines) ? lines : [lines]))
  scrollOutput()
}

function focusInput() {
  void nextTick(() => inputRef.value?.focus())
}

function selectTab(tab: ConsoleTab) {
  activeTab.value = tab
  write(`${prefix.value} open ${tab}`)
  focusInput()
}

function showTutorial() {
  tutorialSlide.value = 0
  tutorialOpen.value = true
}

function closeTutorial() {
  tutorialOpen.value = false
  window.localStorage.setItem(tutorialStorageKey, '1')
  focusInput()
}

function nextTutorialSlide() {
  if (tutorialSlide.value >= tutorialSlides.length - 1) {
    closeTutorial()
    return
  }
  tutorialSlide.value += 1
}

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  const win = getCurrentWindow()
  if (action === 'minimize') await win.minimize()
  if (action === 'toggleMaximize') await win.toggleMaximize()
  if (action === 'close') await win.close()
}

function printStatus() {
  write([
    `hub.status      ${overview.value?.hub.health_status ?? 'booting'}`,
    `hub.running     ${overview.value?.hub.running ? 'true' : 'false'}`,
    `hub.models      ${overview.value?.hub.model_count ?? 0}`,
    `providers       ${filteredProviders.value.length}`,
    `qwen.accounts   ${store.qwenAccounts.length}`,
    `login.windows   ${store.openLoginCount}`,
    `runtime.state   ${runtimeState.value}`,
  ])
}

async function execute(raw = terminalInput.value) {
  const command = raw.trim()
  if (!command) return

  write(`${prefix.value} ${command}`)
  history.value.unshift(command)
  historyIndex.value = -1
  terminalInput.value = ''

  if (command === 'clear') {
    output.value = []
    return
  }

  if (command === 'help') {
    write([
      'Commands:',
      '  status            print runtime and hub status',
      '  refresh           reload dashboard overview',
      '  providers         list visible providers',
      '  tab <name>        open overview, providers, access, qwen, workbench',
      '  tutorial          reopen first-run guide',
      '  clear             clear console output',
    ])
    return
  }

  if (command === 'status') {
    printStatus()
    return
  }

  if (command === 'refresh') {
    await store.refreshOverview()
    store.syncWorkbenchModel()
    write('overview refreshed')
    return
  }

  if (command === 'tutorial') {
    showTutorial()
    return
  }

  if (command.startsWith('tab ')) {
    const tab = command.slice(4).trim() as ConsoleTab
    if (['overview', 'providers', 'access', 'qwen', 'workbench'].includes(tab)) {
      selectTab(tab)
      return
    }
  }

  if (command === 'providers') {
    write(
      filteredProviders.value.length
        ? filteredProviders.value.map(provider => `${provider.name.padEnd(10)} ${String(provider.login_state).padEnd(22)} ${provider.model_count} models`)
        : 'no providers visible'
    )
    return
  }

  write(`command not found: ${command}`)
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    event.preventDefault()
    void execute()
    return
  }
  if (event.key === 'ArrowUp') {
    event.preventDefault()
    if (historyIndex.value + 1 >= history.value.length) return
    historyIndex.value += 1
    terminalInput.value = history.value[historyIndex.value] ?? ''
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
    terminalInput.value = history.value[historyIndex.value] ?? ''
  }
}

onMounted(() => {
  void store.initApp()
  focusInput()
  tutorialOpen.value = window.localStorage.getItem(tutorialStorageKey) !== '1'
  clockTimer = window.setInterval(() => (now.value = new Date()), 1000)
})

onBeforeUnmount(() => {
  store.disposeApp()
  if (clockTimer) window.clearInterval(clockTimer)
})
</script>

<template>
  <div class="operator-console">
    <header class="app-titlebar" data-tauri-drag-region>
      <div class="titlebar-brand" data-tauri-drag-region>
        <HugeiconsIcon :icon="AppWindowIcon" :size="18" aria-hidden="true" />
        <span>RustProxyHub</span>
        <strong>{{ runtimeState }}</strong>
      </div>
      <div class="titlebar-status" data-tauri-drag-region>
        <span>{{ clock }}</span>
        <span>{{ overview?.hub.base_url ?? 'hub offline' }}</span>
      </div>
      <div class="titlebar-actions">
        <button type="button" class="icon-button" title="Open guide" aria-label="Open guide" @click="showTutorial">
          <HugeiconsIcon :icon="HelpCircleIcon" :size="16" aria-hidden="true" />
        </button>
        <button type="button" class="icon-button" title="Minimize" aria-label="Minimize" @click="windowAction('minimize')">
          <HugeiconsIcon :icon="Minimize01Icon" :size="16" aria-hidden="true" />
        </button>
        <button type="button" class="icon-button" title="Maximize or restore" aria-label="Maximize or restore" @click="windowAction('toggleMaximize')">
          <HugeiconsIcon :icon="Maximize01Icon" :size="16" aria-hidden="true" />
        </button>
        <button type="button" class="icon-button danger" title="Close" aria-label="Close" @click="windowAction('close')">
          <HugeiconsIcon :icon="Cancel01Icon" :size="16" aria-hidden="true" />
        </button>
      </div>
    </header>

    <div class="console-body">
      <aside class="rail" aria-label="Console sections">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          type="button"
          class="rail-item"
          :class="{ active: activeTab === tab.key }"
          @click="selectTab(tab.key)"
        >
          <HugeiconsIcon :icon="tab.icon" :size="20" aria-hidden="true" />
          <span>{{ tab.label }}</span>
          <small>{{ tab.meta }}</small>
        </button>
      </aside>

      <main class="workspace">
        <p v-if="notice" class="notice-banner">{{ notice }}</p>
        <p v-if="error" class="error-banner">{{ error }}</p>

        <section class="workspace-panel">
          <HubHeader v-show="activeTab === 'overview'" :overview="overview" />
          <ProviderGrid v-show="activeTab === 'providers'" :providers="filteredProviders" />
          <LoginStudio v-show="activeTab === 'access'" :providers="filteredProviders" />
          <QwenAccountsPanel v-show="activeTab === 'qwen'" />
          <WorkbenchPanel v-show="activeTab === 'workbench'" />
        </section>

        <section class="command-console" aria-label="Command console">
          <div ref="outputRef" class="command-output" aria-live="polite">
            <p v-for="(entry, index) in output" :key="`${entry}-${index}`">{{ entry }}</p>
          </div>
          <label class="command-input">
            <span>{{ prefix }}</span>
            <input ref="inputRef" v-model="terminalInput" autocomplete="off" spellcheck="false" @keydown="handleKeydown" />
          </label>
        </section>
      </main>
    </div>

    <div v-if="tutorialOpen" class="tutorial-backdrop" role="dialog" aria-modal="true" aria-labelledby="tutorial-title">
      <section class="tutorial-dialog">
        <div class="tutorial-icon">
          <HugeiconsIcon :icon="AiChat02Icon" :size="24" aria-hidden="true" />
        </div>
        <p class="kicker">First run</p>
        <h2 id="tutorial-title">{{ tutorialSlides[tutorialSlide].title }}</h2>
        <p>{{ tutorialSlides[tutorialSlide].body }}</p>
        <div class="tutorial-progress" aria-label="Tutorial progress">
          <span v-for="(_, index) in tutorialSlides" :key="index" :class="{ active: index === tutorialSlide }" />
        </div>
        <div class="tutorial-actions">
          <button type="button" class="secondary-button" @click="closeTutorial">Skip</button>
          <button type="button" class="primary-button" @click="nextTutorialSlide">
            {{ tutorialSlide === tutorialSlides.length - 1 ? 'Done' : 'Next' }}
          </button>
        </div>
      </section>
    </div>

    <DetailsDrawer />
  </div>
</template>
