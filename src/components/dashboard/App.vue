<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue'
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'

const store = useStore()
const { overview, error, filteredProviders } = storeToRefs(store)

type TerminalTab = 'overview' | 'providers' | 'access' | 'qwen' | 'workbench'

const activeTab = ref<TerminalTab>('overview')
const terminalInput = ref('')
const inputRef = ref<HTMLInputElement | null>(null)
const outputRef = ref<HTMLElement | null>(null)
const soundEnabled = ref(true)
const history = ref<string[]>([])
const historyIndex = ref(-1)
const output = ref<string[]>([
  'RustProxy shell initialized.',
  "Type 'help' for local commands. Use Ctrl+1..5 to switch panes.",
])

let audioContext: AudioContext | null = null
let clockTimer: number | null = null
const now = ref(new Date())

const tabs = computed(() => [
  { key: 'overview', label: 'overview.sh', meta: overview.value?.hub.health_status ?? 'booting' },
  { key: 'providers', label: 'providers.log', meta: `${filteredProviders.value.length} visible` },
  { key: 'access', label: 'loginctl', meta: `${store.openLoginCount} active` },
  { key: 'qwen', label: 'qwen.vault', meta: `${store.qwenAccounts.length} stored` },
  { key: 'workbench', label: 'probe.ts', meta: store.workbenchModel || 'no model' },
] as const)

const prefix = computed(() => `visitor@rustproxy:${activeTab.value}$`)
const clock = computed(() => now.value.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }))

function playSound(kind: 'key' | 'tab' | 'ok' | 'error' = 'key') {
  if (!soundEnabled.value || typeof window === 'undefined') return
  try {
    audioContext ??= new AudioContext()
    const oscillator = audioContext.createOscillator()
    const gain = audioContext.createGain()
    const frequencies = { key: 520, tab: 680, ok: 860, error: 160 }
    oscillator.type = kind === 'error' ? 'sawtooth' : 'square'
    oscillator.frequency.value = frequencies[kind]
    gain.gain.setValueAtTime(0.0001, audioContext.currentTime)
    gain.gain.exponentialRampToValueAtTime(kind === 'key' ? 0.018 : 0.035, audioContext.currentTime + 0.01)
    gain.gain.exponentialRampToValueAtTime(0.0001, audioContext.currentTime + (kind === 'key' ? 0.045 : 0.09))
    oscillator.connect(gain)
    gain.connect(audioContext.destination)
    oscillator.start()
    oscillator.stop(audioContext.currentTime + 0.11)
  } catch {
    // Audio is decorative only. Ignore browser autoplay restrictions.
  }
}

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

function focusShell(event?: MouseEvent) {
  const target = event?.target as HTMLElement | null
  if (target?.closest('button, a, select, input, textarea, [contenteditable="true"], .terminal-window')) return
  focusInput()
}

function selectTab(tab: TerminalTab) {
  activeTab.value = tab
  playSound('tab')
  write(`${prefix.value} cd ./${tab}`)
}

function printStatus() {
  write([
    `hub.status     ${overview.value?.hub.health_status ?? 'booting'}`,
    `hub.running    ${overview.value?.hub.running ? 'true' : 'false'}`,
    `hub.models     ${overview.value?.hub.model_count ?? 0}`,
    `providers      ${filteredProviders.value.length}`,
    `qwen.accounts  ${store.qwenAccounts.length}`,
    `login.windows  ${store.openLoginCount}`,
    `sounds         ${soundEnabled.value ? 'on' : 'off'}`,
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
    playSound('ok')
    return
  }

  if (command === 'help') {
    write([
      'Available commands:',
      '  help                 print this menu',
      '  clear                clear terminal output',
      '  status               print hub/provider status',
      '  refresh              reload hub overview',
      '  sounds on|off        enable or mute proxy beeps',
      '  tab overview         open overview.sh',
      '  tab providers        open providers.log',
      '  tab access           open loginctl',
      '  tab qwen             open qwen.vault',
      '  tab workbench        open probe.ts',
      '  providers            list visible providers',
    ])
    playSound('ok')
    return
  }

  if (command === 'status') {
    printStatus()
    playSound('ok')
    return
  }

  if (command === 'refresh') {
    await store.refreshOverview()
    write('stdout: overview refreshed')
    playSound('ok')
    return
  }

  if (command === 'sounds on' || command === 'sounds off') {
    soundEnabled.value = command.endsWith('on')
    write(`stdout: proxy sounds ${soundEnabled.value ? 'enabled' : 'muted'}`)
    playSound('ok')
    return
  }

  if (command.startsWith('tab ')) {
    const tab = command.slice(4).trim() as TerminalTab
    if (['overview', 'providers', 'access', 'qwen', 'workbench'].includes(tab)) {
      selectTab(tab)
      return
    }
  }

  if (command === 'providers') {
    const lines = filteredProviders.value.length
      ? filteredProviders.value.map((provider) => `${provider.name.padEnd(10)} ${String(provider.login_state).padEnd(22)} ${provider.model_count} models`)
      : ['stdout: no providers visible']
    write(lines)
    playSound('ok')
    return
  }

  write(`stderr: command not found: ${command}`)
  playSound('error')
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

function handleGlobalClick(event: MouseEvent) {
  const target = event.target as HTMLElement | null
  if (target?.closest('button, a, select, input, textarea')) playSound('key')
}

onMounted(() => {
  void store.initApp()
  focusInput()
  clockTimer = window.setInterval(() => (now.value = new Date()), 1000)
  window.addEventListener('click', handleGlobalClick, true)
})

onBeforeUnmount(() => {
  store.disposeApp()
  if (clockTimer) window.clearInterval(clockTimer)
  window.removeEventListener('click', handleGlobalClick, true)
  void audioContext?.close()
})
</script>

<template>
  <div class="vue-terminal-page">
    <main class="vue-terminal" @click="focusShell">
      <p v-if="error" class="terminal-error"><span>stderr:</span> {{ error }}</p>

      <section class="terminal-window" @click.stop>
        <div class="window-title">
          <span>visitor@rustproxy:/{{ activeTab }}</span>
          <span>{{ clock }}</span>
        </div>

        <HubHeader v-show="activeTab === 'overview'" :overview="overview" />
        <ProviderGrid v-show="activeTab === 'providers'" :providers="filteredProviders" />
        <LoginStudio v-show="activeTab === 'access'" :providers="filteredProviders" />
        <QwenAccountsPanel v-show="activeTab === 'qwen'" />
        <WorkbenchPanel v-show="activeTab === 'workbench'" />
      </section>

      <footer class="rustproxy-bottom-shell" @click="focusShell">
        <div class="terminal-separator">╾──────────────────────────── rustproxyshell :: panes ────────────────────────────╼</div>

        <nav class="terminal-tabs" aria-label="RustProxy terminal panes">
          <button
            v-for="tab in tabs"
            :key="tab.key"
            type="button"
            class="terminal-tab"
            :class="{ active: activeTab === tab.key }"
            @click.stop="selectTab(tab.key)"
          >
            <span>$ {{ tab.label }}</span>
            <small>{{ tab.meta }}</small>
          </button>
        </nav>

        <section class="terminal-output-block">
          <ul ref="outputRef" class="vue-terminal-output-container">
            <li v-for="(entry, index) in output" :key="`${entry}-${index}`">
              <pre v-for="(line, lineIndex) in entry.split('\n')" :key="lineIndex"><span>{{ line }}</span></pre>
            </li>
          </ul>

          <div class="vue-terminal-input-container">
            <div class="vue-terminal-prefix">{{ prefix }}</div>
            <input
              ref="inputRef"
              v-model="terminalInput"
              class="vue-terminal-input"
              autocomplete="off"
              spellcheck="false"
              @keydown="handleKeydown"
            />
          </div>
        </section>
      </footer>
    </main>

    <DetailsDrawer />
  </div>
</template>

<style scoped>

@import url('https://fonts.googleapis.com/css?family=VT323');

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


:global(:root) {
  --vt-font-family: 'VT323', 'JetBrains Mono', Consolas, monospace;
  --vt-font-size: 21px;
  --vt-margin: 22px;
  --vt-padding: 6px;
  --vt-border: 3px solid lightgreen;
  --vt-background-color: #000;
  --vt-color: lightgreen;
}

* { box-sizing: border-box; }

.vue-terminal-page {
  min-height: 100vh;
  width: 100%;
  padding: var(--vt-margin);
  background:
    radial-gradient(circle at 50% 0%, rgba(144, 238, 144, 0.16), transparent 28rem),
    linear-gradient(135deg, #202b28 0%, #0d1412 45%, #000 100%);
  color: var(--vt-color);
  font-family: var(--vt-font-family);
  overflow: hidden;
}

.vue-terminal {
  position: relative;
  width: calc(100vw - 2 * var(--vt-margin));
  height: calc(100vh - 2 * var(--vt-margin));
  padding: var(--vt-padding);
  border: var(--vt-border);
  border-radius: 0;
  background-color: var(--vt-background-color);
  color: var(--vt-color);
  font-family: var(--vt-font-family);
  font-size: var(--vt-font-size);
  display: flex;
  flex-direction: column;
  gap: .65rem;
  overflow: hidden;
  box-shadow: 0 0 0 1px rgba(144, 238, 144, 0.18), 0 0 42px rgba(144, 238, 144, 0.14), inset 0 0 34px rgba(144, 238, 144, 0.08);
}

.vue-terminal::before {
  content: '';
  position: fixed;
  inset: var(--vt-margin);
  pointer-events: none;
  background: repeating-linear-gradient(to bottom, rgba(255,255,255,.05), rgba(255,255,255,.05) 1px, transparent 1px, transparent 4px);
  mix-blend-mode: screen;
  opacity: .24;
  z-index: 5;
}

.rustproxy-bottom-shell {
  flex: 0 0 auto;
  width: 100%;
  border-top: 1px solid rgba(144, 238, 144, .35);
  padding-top: .2rem;
  background: #000;
}

.terminal-output-block {
  width: 100%;
  min-height: 7.5rem;
  max-height: 14rem;
  border: 1px solid rgba(144, 238, 144, 0.35);
  background: #010301;
}

.vue-terminal-output-container {
  width: 100%;
  max-height: 8.5rem;
  overflow: auto;
  list-style: none;
  margin: 0;
  padding: var(--vt-padding);
}

.vue-terminal-output-container li {
  width: 100%;
  padding: 0 var(--vt-padding);
}

.vue-terminal-output-container li pre {
  width: 100%;
  margin: 0;
  padding: 0;
}

.vue-terminal-output-container li pre span {
  width: 100%;
  font-family: var(--vt-font-family);
  font-size: var(--vt-font-size);
  white-space: pre;
}

.vue-terminal-input-container {
  width: 100%;
  display: flex;
  align-items: center;
  border-top: 1px solid rgba(144, 238, 144, 0.25);
  padding: 0 var(--vt-padding) var(--vt-padding);
}

.vue-terminal-prefix {
  padding: var(--vt-padding);
  padding-bottom: 0;
  flex-shrink: 0;
}

.vue-terminal-input {
  flex: 1;
  min-width: 5rem;
  border: 0;
  outline: none;
  background: transparent;
  font-family: var(--vt-font-family);
  font-size: var(--vt-font-size);
  color: transparent;
  text-shadow: 0 0 0 var(--vt-color);
  caret-color: transparent;
  padding: 0;
  margin: var(--vt-padding);
  margin-bottom: 0;
  border-right: 10px solid var(--vt-color);
  animation: terminal-cursor 1s steps(1) infinite;
}

@keyframes terminal-cursor { 50% { border-right-color: transparent; } }

.terminal-separator {
  padding: .25rem 0 .35rem;
  color: rgba(144, 238, 144, .72);
  white-space: nowrap;
  overflow: hidden;
}

.terminal-tabs {
  width: 100%;
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: .45rem;
  margin-bottom: .55rem;
}

.terminal-tab {
  border: 1px solid rgba(144, 238, 144, .38);
  border-radius: 0;
  background: #030903;
  color: var(--vt-color);
  padding: .5rem .65rem;
  font: inherit;
  text-align: center;
  cursor: pointer;
}

.terminal-tab.active,
.terminal-tab:hover {
  background: rgba(144, 238, 144, .16);
  box-shadow: inset 0 0 0 1px rgba(144, 238, 144, .25);
}

.terminal-tab span,
.terminal-tab small {
  display: block;
}

.terminal-tab small {
  color: rgba(144, 238, 144, .68);
  font-size: .82em;
}

.terminal-error {
  margin: 0 0 .7rem;
  border: 1px solid rgba(255, 82, 82, .55);
  padding: .55rem .7rem;
  color: #ffb4b4;
  background: rgba(80,0,0,.26);
}

.terminal-error span { color: #ff6767; }

.terminal-window {
  flex: 1 1 auto;
  width: 100%;
  min-width: 0;
  min-height: 0;
  overflow: auto;
  border: 1px solid rgba(144, 238, 144, .42);
  background: #010401;
}

.window-title {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  padding: .45rem .65rem;
  border-bottom: 1px solid rgba(144, 238, 144, .35);
  background: rgba(144, 238, 144, .12);
}

:deep(.terminal-panel),
:deep(.panel) {
  width: 100%;
  min-width: 0;
  border: 0;
  border-radius: 0 !important;
  background: #010401 !important;
  box-shadow: none !important;
}

:deep(button),
:deep(input),
:deep(select),
:deep(textarea) {
  border-radius: 0 !important;
  font-family: var(--vt-font-family) !important;
}


:deep(label),
:deep(input),
:deep(select),
:deep(textarea),
:deep(button),
:deep(a) {
  pointer-events: auto;
}

:deep(input),
:deep(select),
:deep(textarea) {
  user-select: text;
}

@media (max-width: 920px) {
  .vue-terminal-page { padding: .65rem; }
  .vue-terminal { width: calc(100vw - 1.3rem); height: calc(100vh - 1.3rem); }
  .terminal-tabs { grid-template-columns: 1fr; }
  .terminal-output-block { min-height: 7rem; }
}
</style>
