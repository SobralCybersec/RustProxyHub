<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  ArrowExpand01Icon,
  CheckmarkBadge01Icon,
  CpuIcon,
  Login03Icon,
  PlayIcon,
  RoboticIcon,
} from '@hugeicons/core-free-icons'
import { computed } from 'vue'
import SparkChart from '@/components/dashboard/SparkChart.vue'
import { burstFromElement, playTone } from '@/effects'
import type { ProviderName, ProviderOverview } from '@/lib/types'
import { useStore } from '@/store'

const props = defineProps<{ providers: ProviderOverview[] }>()
const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const summary = computed(() => {
  const live = props.providers.filter((p) => p.running)
  return {
    live: live.length,
    models: props.providers.reduce((sum, p) => sum + p.model_count, 0),
    authenticated: props.providers.filter((p) => p.login_state === 'authenticated').length,
  }
})

function statusLabel(p: ProviderOverview) {
  if (!p.running) return t('providers.status.idle')
  if (p.health_status === 'degraded') return t('providers.status.warn')
  if (p.login_state === 'authenticated') return t('providers.status.ok')
  return t('providers.status.run')
}
function statusClass(p: ProviderOverview) {
  if (!p.running) return 'idle'
  if (p.health_status === 'degraded') return 'warn'
  return 'ok'
}
// ponytail: deterministic stable curve derived from rpm; no Math.random so the
// SparkChart watcher stops re-rendering on every parent re-paint.
const TOKEN_SERIES_CACHE = new WeakMap<ProviderOverview, { rpm: number | undefined; values: number[] }>()
function tokenSeries(p: ProviderOverview): number[] {
  const cached = TOKEN_SERIES_CACHE.get(p)
  if (cached && cached.rpm === p.rpm) return cached.values
  const base = p.rpm ?? 20
  const values = Array.from({ length: 12 }, (_, i) => base * (0.7 + Math.sin((i + base) * 0.6) * 0.3))
  TOKEN_SERIES_CACHE.set(p, { rpm: p.rpm, values })
  return values
}

function onStart(event: MouseEvent, name: ProviderName) {
  burstFromElement(event.currentTarget as Element, '#ff9500', 14)
  playTone('boot')
  void store.startService(name)
}
function onLogin(event: MouseEvent, p: ProviderOverview) {
  burstFromElement(event.currentTarget as Element, '#5ce1e6', 12)
  void store.startProviderLogin(p.name)
}
function copyAgent(name: ProviderName, agent: 'pi' | 'kilo' | 'claude') {
  void store.copyAgentSetup(name, agent)
}
</script>

<template>
  <div class="provider-panel">
    <header class="panel-head">
      <div>
        <span class="eyebrow">{{ t('providers.kicker') }}</span>
        <h2 class="panel-title">{{ t('providers.title') }}</h2>
      </div>
      <div class="summary-pills">
        <div class="summary-pill">
          <span class="pill-num">{{ summary.live }}</span>
          <span class="pill-label">{{ t('providers.summary.live') }}</span>
        </div>
        <div class="summary-pill">
          <span class="pill-num">{{ summary.models }}</span>
          <span class="pill-label">{{ t('providers.summary.models') }}</span>
        </div>
        <div class="summary-pill">
          <span class="pill-num">{{ summary.authenticated }}</span>
          <span class="pill-label">{{ t('providers.summary.authenticated') }}</span>
        </div>
      </div>
    </header>

    <p class="panel-copy">{{ t('providers.copy') }}</p>

    <div v-if="!providers.length" class="empty-state">{{ t('providers.empty') }}</div>

    <div v-else class="provider-grid">
      <article v-for="p in providers" :key="p.name" v-tilt class="card provider-card" :class="statusClass(p)">
        <div class="provider-card-glow" aria-hidden="true" />
        <header class="provider-card-head">
          <div class="provider-id">
            <span class="provider-avatar"><HugeiconsIcon :icon="RoboticIcon" :size="20" aria-hidden="true" /></span>
            <div>
              <h3 class="provider-name">{{ p.name }}</h3>
              <span class="provider-url">{{ p.base_url || t('providers.noBaseUrl') }}</span>
            </div>
          </div>
          <span class="status-chip" :class="statusClass(p)">
            <span class="chip-dot" />{{ statusLabel(p) }}
          </span>
        </header>

        <div class="provider-metrics">
          <div class="pm">
            <span class="pm-label">LAT</span>
            <span class="pm-value">{{ p.running ? `${p.latency_ms ?? 0}ms` : '--' }}</span>
          </div>
          <div class="pm">
            <span class="pm-label">RPM</span>
            <span class="pm-value">{{ p.running ? p.rpm ?? 0 : '--' }}</span>
          </div>
          <div class="pm">
            <span class="pm-label">ERR</span>
            <span class="pm-value" :class="{ danger: (p.error_rate ?? 0) > 3 }">
              {{ p.running ? `${(p.error_rate ?? 0).toFixed(1)}%` : '--' }}
            </span>
          </div>
          <div class="pm">
            <span class="pm-label">UP</span>
            <span class="pm-value">{{ p.running ? `${(p.uptime_pct ?? 0).toFixed(1)}%` : '--' }}</span>
          </div>
        </div>

        <SparkChart
          v-if="p.running"
          :values="tokenSeries(p)"
          :height="32"
          :stroke="statusClass(p) === 'warn' ? '#ff9500' : '#5ce1e6'"
          :fill="statusClass(p) === 'warn' ? 'rgba(255,149,0,0.14)' : 'rgba(92,225,230,0.14)'"
        />
        <div v-else class="spark-placeholder">{{ t('providers.modelBufferEmpty') }}</div>

        <div class="provider-models">
          <HugeiconsIcon :icon="CpuIcon" :size="13" aria-hidden="true" />
          <template v-if="p.models.length">
            <span v-for="m in p.models.slice(0, 2)" :key="m" class="model-tag">{{ m }}</span>
            <span v-if="p.models.length > 2" class="model-tag more">
              {{ t('providers.modelExtras', { count: p.models.length - 2 }) }}
            </span>
          </template>
          <span v-else class="model-empty">{{ t('providers.modelBufferEmpty') }}</span>
        </div>

        <footer class="provider-actions">
          <button
            v-if="!p.running"
            type="button"
            class="pa-btn primary"
            :disabled="store.isBusy(`service:start:${p.name}`)"
            @click="onStart($event, p.name)"
          >
            <HugeiconsIcon :icon="PlayIcon" :size="13" aria-hidden="true" />
            {{ t('providers.actions.start') }}
          </button>
          <button
            v-else
            type="button"
            class="pa-btn"
            :disabled="store.isBusy(`login:start:${p.name}`)"
            @click="onLogin($event, p)"
          >
            <HugeiconsIcon :icon="p.login_state === 'authenticated' ? CheckmarkBadge01Icon : Login03Icon" :size="13" aria-hidden="true" />
            {{ p.login_state === 'authenticated' ? t('providers.actions.relogin') : t('providers.actions.login') }}
          </button>

          <button type="button" class="pa-btn ghost" @click="copyAgent(p.name, 'pi')">{{ t('providers.actions.pi') }}</button>
          <button type="button" class="pa-btn ghost" @click="copyAgent(p.name, 'claude')">{{ t('providers.actions.claude') }}</button>
          <button type="button" class="pa-btn icon" :title="t('providers.actions.dossier')" :aria-label="t('providers.actions.dossier')" @click="store.openProviderDrawer(p.name)">
            <HugeiconsIcon :icon="ArrowExpand01Icon" :size="14" aria-hidden="true" />
          </button>
        </footer>
      </article>
    </div>
  </div>
</template>

<style scoped>
.provider-panel {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
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
  font-family: var(--font-arcade);
  font-size: 0.5rem;
  color: var(--orange);
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
.panel-title {
  font-family: var(--font-arcade);
  font-size: 0.88rem;
  margin-top: 0.5rem;
  color: var(--text);
}
.panel-copy {
  color: var(--muted-strong);
  font-size: 0.75rem;
  line-height: 1.55;
  max-width: 60ch;
}
.summary-pills {
  display: flex;
  gap: 0.5rem;
}
.summary-pill {
  display: flex;
  flex-direction: column;
  align-items: center;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.45rem 0.7rem;
  background: var(--void-2);
  min-width: 3.6rem;
}
.pill-num {
  font-family: var(--font-arcade);
  font-size: 0.85rem;
  color: var(--orange);
  text-shadow: var(--glow-orange);
}
.pill-label {
  font-size: 0.54rem;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-top: 0.2rem;
}

.empty-state {
  border: 1px dashed var(--line-strong);
  border-radius: var(--radius-md);
  padding: 2.5rem 1rem;
  text-align: center;
  color: var(--muted);
  font-size: 0.78rem;
}

.provider-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
  gap: 0.8rem;
}
.provider-card {
  position: relative;
  padding: 0.95rem;
  display: flex;
  flex-direction: column;
  gap: 0.7rem;
  transition: border-color 0.18s ease, box-shadow 0.18s ease;
}
.provider-card.ok {
  border-color: rgba(92, 225, 230, 0.25);
}
.provider-card.warn {
  border-color: rgba(255, 149, 0, 0.4);
}
.provider-card.idle {
  opacity: 0.82;
}
.provider-card-glow {
  position: absolute;
  inset: 0;
  background: radial-gradient(circle at 80% -10%, rgba(255, 149, 0, 0.1), transparent 55%);
  pointer-events: none;
}
.provider-card.ok .provider-card-glow {
  background: radial-gradient(circle at 80% -10%, rgba(92, 225, 230, 0.1), transparent 55%);
}

.provider-card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 0.6rem;
}
.provider-id {
  display: flex;
  gap: 0.55rem;
  align-items: center;
}
.provider-avatar {
  display: grid;
  place-items: center;
  width: 2.1rem;
  height: 2.1rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-sm);
  color: var(--orange);
  background: var(--void-2);
}
.provider-name {
  font-size: 0.84rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--text);
}
.provider-url {
  font-size: 0.58rem;
  color: var(--muted);
}
.status-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: 0.56rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  padding: 0.3rem 0.5rem;
  border-radius: 2px;
  border: 1px solid var(--line);
  white-space: nowrap;
}
.status-chip .chip-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--muted);
}
.status-chip.ok {
  color: var(--cyan);
  border-color: rgba(92, 225, 230, 0.4);
}
.status-chip.ok .chip-dot {
  background: var(--cyan);
  box-shadow: 0 0 6px var(--cyan);
}
.status-chip.warn {
  color: var(--orange);
  border-color: rgba(255, 149, 0, 0.5);
}
.status-chip.warn .chip-dot {
  background: var(--orange);
  box-shadow: 0 0 6px var(--orange);
}
.status-chip.idle {
  color: var(--muted);
}

.provider-metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.4rem;
}
.pm {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  border: 1px solid var(--line-soft);
  border-radius: 2px;
  padding: 0.35rem 0.4rem;
  background: rgba(0, 0, 0, 0.25);
}
.pm-label {
  font-size: 0.5rem;
  color: var(--muted);
  letter-spacing: 0.08em;
}
.pm-value {
  font-family: var(--font-arcade);
  font-size: 0.62rem;
  color: var(--text);
}
.pm-value.danger {
  color: var(--danger);
}

.spark-placeholder {
  height: 32px;
  display: grid;
  place-items: center;
  font-size: 0.58rem;
  color: var(--muted);
  border: 1px dashed var(--line-soft);
  border-radius: 2px;
}

.provider-models {
  display: flex;
  align-items: center;
  gap: 0.35rem;
  flex-wrap: wrap;
  color: var(--muted);
}
.model-tag {
  font-size: 0.56rem;
  border: 1px solid var(--line);
  border-radius: 2px;
  padding: 0.2rem 0.4rem;
  color: var(--muted-strong);
  background: var(--void-2);
}
.model-tag.more {
  color: var(--orange);
  border-color: var(--line-strong);
}
.model-empty {
  font-size: 0.58rem;
  color: var(--muted);
}

.provider-actions {
  display: flex;
  gap: 0.35rem;
  flex-wrap: wrap;
  margin-top: auto;
}
.pa-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  border: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.02);
  color: var(--muted-strong);
  padding: 0.42rem 0.55rem;
  cursor: pointer;
  font-size: 0.58rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border-radius: var(--radius-sm);
  transition: all 0.15s ease;
}
.pa-btn:hover:not(:disabled) {
  color: var(--orange);
  border-color: var(--orange);
  background: var(--orange-soft);
}
.pa-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.pa-btn.primary {
  color: var(--void);
  background: var(--orange);
  border-color: var(--orange);
  font-weight: 700;
}
.pa-btn.primary:hover:not(:disabled) {
  box-shadow: var(--glow-orange);
  color: var(--void);
}
.pa-btn.ghost {
  font-size: 0.54rem;
}
.pa-btn.icon {
  margin-left: auto;
  padding: 0.42rem 0.5rem;
}

@media (max-width: 520px) {
  .provider-grid {
    grid-template-columns: 1fr;
  }
}
</style>
