<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  Cancel01Icon,
  CheckmarkCircle02Icon,
  CpuIcon,
  Database02Icon,
  DashboardSpeed02Icon,
  Globe02Icon,
  InformationCircleIcon,
  PulseIcon,
  RefreshIcon,
  ServerStack01Icon,
} from '@hugeicons/core-free-icons'
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import type { ProviderDetails } from '@/lib/types'
import { useStore } from '@/store'

const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const providerName = computed(() => store.activeDrawer)
const details = computed<ProviderDetails | null>(() => store.activeProviderDetails)
const overview = computed(() => details.value?.overview ?? null)
const detailRows = computed(() => {
  const detail = details.value?.detail ?? {}
  return Object.entries(detail).map(([key, value]) => ({ key, value: typeof value === 'boolean' ? (value ? 'true' : 'false') : String(value) }))
})
const logs = computed(() => store.activeProviderLogs)
const models = computed(() => overview.value?.models ?? [])
const qwenAccounts = computed(() => details.value?.qwen_accounts ?? [])
const isLoading = computed(() => (providerName.value ? store.isBusy(`drawer:${providerName.value}`) : false))

const metrics = computed(() => [
  { key: 'latency', icon: DashboardSpeed02Icon, label: 'latency', value: overview.value?.running ? `${overview.value.latency_ms ?? 0}ms` : '--' },
  { key: 'rpm', icon: PulseIcon, label: 'rpm', value: overview.value?.running ? String(overview.value.rpm ?? 0) : '--' },
  { key: 'models', icon: CpuIcon, label: 'models', value: String(overview.value?.model_count ?? 0) },
  { key: 'tokens', icon: Database02Icon, label: 'tokens', value: formatCompact(overview.value?.tokens ?? 0) },
])

function formatCompact(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
  return String(Math.round(value))
}

function startedLabel() {
  const startedAt = overview.value?.started_at
  if (!startedAt) return '--'
  const seconds = Math.max(0, Math.floor(Date.now() / 1000) - startedAt)
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`
}

function close() {
  store.closeProviderDrawer()
}
function refresh() {
  if (providerName.value) void store.openProviderDrawer(providerName.value)
}

// --- focus trap + escape ---
const drawerEl = ref<HTMLElement | null>(null)
let previousFocus: Element | null = null

function getFocusable(): HTMLElement[] {
  if (!drawerEl.value) return []
  return Array.from(
    drawerEl.value.querySelectorAll<HTMLElement>(
      'button:not(:disabled), [href], input:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  )
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') { close(); return }
  if (e.key !== 'Tab') return
  const focusable = getFocusable()
  if (!focusable.length) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.shiftKey) {
    if (document.activeElement === first) { e.preventDefault(); last.focus() }
  } else {
    if (document.activeElement === last) { e.preventDefault(); first.focus() }
  }
}

watch(providerName, async (name, old) => {
  if (name && !old) {
    previousFocus = document.activeElement
    await nextTick()
    getFocusable()[0]?.focus()
    document.addEventListener('keydown', onKeydown)
  } else if (!name) {
    document.removeEventListener('keydown', onKeydown)
    ;(previousFocus as HTMLElement | null)?.focus()
    previousFocus = null
  }
})

onUnmounted(() => document.removeEventListener('keydown', onKeydown))
</script>

<template>
  <Teleport to="body">
    <Transition name="drawer-fade">
      <div v-if="providerName" class="drawer-backdrop" @click.self="close">
        <aside ref="drawerEl" class="details-drawer" role="dialog" aria-modal="true" :aria-label="`${providerName} ${t('details.inspect')}`">
          <header class="drawer-head">
            <div class="drawer-title-block">
              <span class="drawer-kicker"><HugeiconsIcon :icon="InformationCircleIcon" :size="12" aria-hidden="true" /> {{ t('details.inspect') }}</span>
              <h2>{{ providerName }}</h2>
              <p>{{ overview?.base_url || t('providers.noBaseUrl') }}</p>
            </div>
            <div class="drawer-actions">
              <button type="button" class="drawer-icon-btn" aria-label="Refresh provider details" :disabled="isLoading" @click="refresh">
                <HugeiconsIcon :icon="RefreshIcon" :size="15" aria-hidden="true" />
              </button>
              <button type="button" class="drawer-icon-btn danger" :aria-label="t('details.close')" @click="close">
                <HugeiconsIcon :icon="Cancel01Icon" :size="15" aria-hidden="true" />
              </button>
            </div>
          </header>

          <div v-if="isLoading && !details" class="drawer-loading">loading dossier...</div>

          <template v-else>
            <section class="drawer-state card">
              <div class="state-chip" :class="{ on: overview?.running, warn: overview?.health_status === 'degraded' }">
                <span class="state-dot" />
                {{ overview?.running ? overview?.health_status : t('common.offline') }}
              </div>
              <div class="state-meta">
                <span><HugeiconsIcon :icon="ServerStack01Icon" :size="13" aria-hidden="true" /> {{ startedLabel() }}</span>
                <span><HugeiconsIcon :icon="Globe02Icon" :size="13" aria-hidden="true" /> {{ overview?.login_state ?? '--' }}</span>
                <span v-if="overview?.web_search_supported"><HugeiconsIcon :icon="CheckmarkCircle02Icon" :size="13" aria-hidden="true" /> web search</span>
              </div>
            </section>

            <section class="metric-grid">
              <article v-for="metric in metrics" :key="metric.key" class="card drawer-metric">
                <HugeiconsIcon :icon="metric.icon" :size="15" aria-hidden="true" />
                <span>{{ metric.label }}</span>
                <strong>{{ metric.value }}</strong>
              </article>
            </section>

            <section class="drawer-section card">
              <div class="section-title">models</div>
              <div v-if="models.length" class="model-list">
                <span v-for="model in models" :key="model" class="model-chip">{{ model }}</span>
              </div>
              <p v-else class="muted-line">{{ t('providers.modelBufferEmpty') }}</p>
            </section>

            <section v-if="detailRows.length" class="drawer-section card">
              <div class="section-title">detail</div>
              <div class="detail-table">
                <div v-for="row in detailRows" :key="row.key" class="detail-row">
                  <span>{{ row.key }}</span>
                  <strong>{{ row.value }}</strong>
                </div>
              </div>
            </section>

            <section v-if="qwenAccounts.length" class="drawer-section card">
              <div class="section-title">qwen accounts</div>
              <div class="account-mini-list">
                <div v-for="account in qwenAccounts" :key="account.id" class="account-mini">
                  <span>{{ account.email }}</span>
                  <strong>{{ account.has_password ? t('qwen.passwordSet') : t('qwen.manualOnly') }}</strong>
                </div>
              </div>
            </section>

            <section class="drawer-section card logs-card">
              <div class="section-title">logs</div>
              <pre v-if="logs.length">{{ logs.join('\n') }}</pre>
              <p v-else class="muted-line">{{ t('details.noLogs') }}</p>
            </section>
          </template>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 80;
  display: flex;
  justify-content: flex-end;
  background: rgba(0, 0, 0, 0.56);
  backdrop-filter: blur(4px);
}
.details-drawer {
  width: min(30rem, 94vw);
  height: 100%;
  overflow: auto;
  border-left: 1px solid var(--line-strong);
  background:
    radial-gradient(circle at 100% 0%, rgba(255, 149, 0, 0.12), transparent 32%),
    linear-gradient(180deg, var(--panel-2), var(--void));
  box-shadow: -20px 0 70px rgba(0, 0, 0, 0.55);
  padding: 1rem;
}
.drawer-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
  margin-bottom: 0.9rem;
}
.drawer-kicker {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-family: var(--font-arcade);
  font-size: 0.48rem;
  color: var(--orange);
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
.drawer-title-block h2 {
  margin: 0.5rem 0 0.15rem;
  font-family: var(--font-arcade);
  font-size: 1rem;
  color: var(--text);
  text-transform: uppercase;
}
.drawer-title-block p {
  color: var(--muted);
  font-size: 0.68rem;
  word-break: break-all;
}
.drawer-actions {
  display: flex;
  gap: 0.35rem;
}
.drawer-icon-btn {
  display: grid;
  place-items: center;
  width: 2rem;
  height: 2rem;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted-strong);
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.drawer-icon-btn:hover:not(:disabled) {
  color: var(--orange);
  border-color: var(--orange);
}
.drawer-icon-btn.danger:hover {
  color: var(--danger);
  border-color: var(--danger);
}
.drawer-icon-btn:disabled { opacity: 0.45; cursor: not-allowed; }
.drawer-loading {
  border: 1px dashed var(--line-strong);
  border-radius: var(--radius-md);
  color: var(--muted-strong);
  padding: 2rem;
  text-align: center;
}
.drawer-state,
.drawer-section,
.drawer-metric {
  padding: 0.85rem;
}
.drawer-state {
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  margin-bottom: 0.75rem;
}
.state-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  width: fit-content;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.35rem 0.55rem;
  color: var(--muted-strong);
  text-transform: uppercase;
  font-size: 0.58rem;
  letter-spacing: 0.08em;
}
.state-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--muted);
}
.state-chip.on {
  color: var(--cyan);
  border-color: rgba(92, 225, 230, 0.38);
}
.state-chip.on .state-dot { background: var(--cyan); box-shadow: var(--glow-cyan); }
.state-chip.warn {
  color: var(--orange);
  border-color: rgba(255, 149, 0, 0.45);
}
.state-chip.warn .state-dot { background: var(--orange); box-shadow: var(--glow-orange); }
.state-meta {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
}
.state-meta span {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  color: var(--muted-strong);
  border: 1px solid var(--line-soft);
  border-radius: var(--radius-sm);
  padding: 0.32rem 0.45rem;
  font-size: 0.58rem;
}
.metric-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 0.55rem;
  margin-bottom: 0.75rem;
}
.drawer-metric {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.drawer-metric svg { color: var(--orange); }
.drawer-metric span,
.section-title {
  color: var(--muted);
  font-size: 0.54rem;
  text-transform: uppercase;
  letter-spacing: 0.09em;
}
.drawer-metric strong {
  font-family: var(--font-arcade);
  color: var(--text);
  font-size: 0.75rem;
}
.drawer-section {
  margin-bottom: 0.75rem;
}
.section-title {
  margin-bottom: 0.6rem;
  color: var(--orange);
}
.model-list {
  display: flex;
  gap: 0.35rem;
  flex-wrap: wrap;
}
.model-chip {
  border: 1px solid var(--line);
  border-radius: 2px;
  background: var(--void-2);
  color: var(--muted-strong);
  padding: 0.28rem 0.42rem;
  font-size: 0.58rem;
}
.detail-table,
.account-mini-list {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}
.detail-row,
.account-mini {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 0.75rem;
  border-bottom: 1px dashed var(--line-soft);
  padding-bottom: 0.35rem;
  color: var(--muted-strong);
  font-size: 0.64rem;
}
.detail-row span { color: var(--muted); }
.detail-row strong,
.account-mini strong { color: var(--text); font-size: 0.62rem; }
.logs-card pre {
  max-height: 12rem;
  overflow: auto;
  margin: 0;
  white-space: pre-wrap;
  color: var(--cyan);
  background: var(--void-2);
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.7rem;
  font-size: 0.62rem;
  line-height: 1.45;
}
.muted-line {
  color: var(--muted);
  font-size: 0.68rem;
}
.drawer-fade-enter-active,
.drawer-fade-leave-active { transition: opacity 0.2s ease; }
.drawer-fade-enter-active .details-drawer,
.drawer-fade-leave-active .details-drawer { transition: transform 0.2s ease; }
.drawer-fade-enter-from,
.drawer-fade-leave-to { opacity: 0; }
.drawer-fade-enter-from .details-drawer,
.drawer-fade-leave-to .details-drawer { transform: translateX(2rem); }
@media (max-width: 520px) {
  .metric-grid { grid-template-columns: 1fr; }
}
</style>
