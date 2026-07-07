<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  Activity03Icon,
  CheckmarkCircle02Icon,
  Clock01Icon,
  CpuIcon,
  Database02Icon,
  DashboardSpeed02Icon,
  FlashIcon,
  GlobeIcon,
  PulseIcon,
  ServerStack01Icon,
  ShieldEnergyIcon,
} from '@hugeicons/core-free-icons'
import { computed } from 'vue'
import SparkChart from '@/components/dashboard/SparkChart.vue'
import type { DashboardOverview } from '@/lib/types'
import { useStore } from '@/store'

const props = defineProps<{ overview: DashboardOverview | null }>()
const store = useStore()
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const hub = computed(() => props.overview?.hub ?? null)
const providers = computed(() => props.overview?.providers ?? [])
const runtime = computed(() => props.overview?.runtime ?? null)

const liveProviders = computed(() => providers.value.filter((p) => p.running))
const healthyCount = computed(() => liveProviders.value.filter((p) => p.health_status === 'healthy').length)
const totalModels = computed(() => providers.value.reduce((sum, p) => sum + p.model_count, 0))

const totalRpm = computed(() => liveProviders.value.reduce((sum, p) => sum + (p.rpm ?? 0), 0))
const totalTokens = computed(() => liveProviders.value.reduce((sum, p) => sum + (p.tokens ?? 0), 0))
const avgLatency = computed(() => {
  if (!liveProviders.value.length) return 0
  return Math.round(liveProviders.value.reduce((sum, p) => sum + (p.latency_ms ?? 0), 0) / liveProviders.value.length)
})
const avgUptime = computed(() => {
  if (!liveProviders.value.length) return 0
  return liveProviders.value.reduce((sum, p) => sum + (p.uptime_pct ?? 0), 0) / liveProviders.value.length
})
const avgErrorRate = computed(() => {
  if (!liveProviders.value.length) return 0
  return liveProviders.value.reduce((sum, p) => sum + (p.error_rate ?? 0), 0) / liveProviders.value.length
})

const throughputSeries = computed(() => props.overview?.throughput_series ?? [])
const latencySeries = computed(() => props.overview?.latency_series ?? [])

const uptimeSeconds = computed(() => {
  const startedAt = hub.value?.started_at
  if (!startedAt) return 0
  return Math.max(0, Math.floor(Date.now() / 1000) - startedAt)
})
const uptimeLabel = computed(() => {
  const s = uptimeSeconds.value
  if (!s) return '--:--:--'
  const h = String(Math.floor(s / 3600)).padStart(2, '0')
  const m = String(Math.floor((s % 3600) / 60)).padStart(2, '0')
  const sec = String(s % 60).padStart(2, '0')
  return `${h}:${m}:${sec}`
})

function formatNumber(value: number) {
  return Math.round(value).toLocaleString(store.localeCode)
}
function formatCompact(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`
  return String(Math.round(value))
}

const stats = computed(() => [
  {
    key: 'throughput',
    icon: FlashIcon,
    label: t('hub.stats.throughput'),
    value: formatNumber(totalRpm.value),
    unit: 'req/min',
    series: throughputSeries.value,
    accent: 'orange',
  },
  {
    key: 'latency',
    icon: DashboardSpeed02Icon,
    label: t('hub.stats.latency'),
    value: String(avgLatency.value),
    unit: 'ms avg',
    series: latencySeries.value,
    accent: 'cyan',
  },
  {
    key: 'tokens',
    icon: Database02Icon,
    label: t('hub.stats.tokens'),
    value: formatCompact(totalTokens.value),
    unit: 'served',
    series: liveProviders.value.map((p) => p.tokens ?? 0),
    accent: 'orange',
  },
  {
    key: 'models',
    icon: CpuIcon,
    label: t('hub.stats.models'),
    value: String(totalModels.value),
    unit: 'discovered',
    series: providers.value.map((p) => p.model_count),
    accent: 'cyan',
  },
])

const gauges = computed(() => [
  { key: 'uptime', label: t('hub.gauges.uptime'), value: avgUptime.value, max: 100, suffix: '%', good: 'high' },
  { key: 'health', label: t('hub.gauges.health'), value: healthyCount.value, max: Math.max(providers.value.length, 1), suffix: ` / ${providers.value.length}`, good: 'high' },
  { key: 'errors', label: t('hub.gauges.errors'), value: avgErrorRate.value, max: 6, suffix: '%', good: 'low' },
])

function gaugePct(g: { value: number; max: number }) {
  return Math.max(0, Math.min(100, (g.value / g.max) * 100))
}
function gaugeColor(g: { good: string; value: number; max: number }) {
  const pct = gaugePct(g)
  if (g.good === 'low') return pct < 40 ? 'var(--cyan)' : pct < 70 ? 'var(--orange)' : 'var(--danger)'
  return pct > 70 ? 'var(--cyan)' : pct > 40 ? 'var(--orange)' : 'var(--danger)'
}
</script>

<template>
  <div class="hub-panel">
    <header class="panel-head">
      <div class="panel-head-main">
        <span class="eyebrow">{{ t('hubHeader.eyebrow') }}</span>
        <h2 class="panel-title">{{ t('hubHeader.title') }}</h2>
        <p class="panel-copy">{{ t('hubHeader.copy') }}</p>
      </div>
      <div class="hub-state" :class="{ online: hub?.running }">
        <span class="state-dot" />
        <div class="hub-state-text">
          <strong>{{ hub?.running ? t('common.online') : t('common.offline') }}</strong>
          <span>{{ hub?.base_url ?? '--' }}</span>
        </div>
      </div>
    </header>

    <div class="runtime-row" :class="{ ok: runtime?.single_runner_ready, bad: !runtime?.single_runner_ready }">
      <HugeiconsIcon :icon="runtime?.single_runner_ready ? CheckmarkCircle02Icon : ShieldEnergyIcon" :size="16" aria-hidden="true" />
      <span>{{ runtime?.single_runner_ready ? t('hubHeader.runtimeOk') : t('hubHeader.runtimeFail') }}</span>
      <span class="runtime-uptime">
        <HugeiconsIcon :icon="Clock01Icon" :size="14" aria-hidden="true" />
        {{ uptimeLabel }}
      </span>
    </div>

    <template v-if="runtime && !runtime.single_runner_ready && runtime.issues.length">
      <ul class="runtime-issues">
        <li v-for="issue in runtime.issues" :key="issue" class="runtime-issue">{{ issue }}</li>
      </ul>
      <pre class="runtime-snippet">{{ '{\n  "startup_mode": "manual"\n}' }}</pre>
    </template>

    <div class="stat-grid">
      <article v-for="stat in stats" :key="stat.key" v-tilt class="card stat-tile" :class="`accent-${stat.accent}`">
        <div class="stat-tile-head">
          <HugeiconsIcon :icon="stat.icon" :size="18" aria-hidden="true" />
          <span class="stat-label">{{ stat.label }}</span>
        </div>
        <div class="stat-value-row">
          <span class="stat-value">{{ stat.value }}</span>
          <span class="stat-unit">{{ stat.unit }}</span>
        </div>
        <SparkChart
          :values="stat.series"
          :height="34"
          :stroke="stat.accent === 'cyan' ? '#5ce1e6' : '#ff9500'"
          :fill="stat.accent === 'cyan' ? 'rgba(92,225,230,0.16)' : 'rgba(255,149,0,0.16)'"
        />
      </article>
    </div>

    <div class="hub-lower">
      <section class="card chart-card">
        <div class="chart-card-head">
          <HugeiconsIcon :icon="Activity03Icon" :size="16" aria-hidden="true" />
          <span>{{ t('hub.throughputTitle') }}</span>
          <span class="chart-card-peak">peak {{ formatNumber(Math.max(0, ...throughputSeries)) }}</span>
        </div>
        <SparkChart
          :values="throughputSeries"
          :height="120"
          mode="bars"
          stroke="rgba(255,149,0,0.7)"
        />
      </section>

      <section class="card gauge-card">
        <div class="chart-card-head">
          <HugeiconsIcon :icon="PulseIcon" :size="16" aria-hidden="true" />
          <span>{{ t('hub.healthTitle') }}</span>
        </div>
        <div class="gauges">
          <div v-for="g in gauges" :key="g.key" class="gauge">
            <div class="gauge-top">
              <span class="gauge-label">{{ g.label }}</span>
              <span class="gauge-value">{{ g.value.toFixed(g.key === 'health' ? 0 : g.key === 'errors' ? 2 : 1) }}{{ g.suffix }}</span>
            </div>
            <div class="gauge-track">
              <div class="gauge-fill" :style="{ width: `${gaugePct(g)}%`, background: gaugeColor(g) }" />
            </div>
          </div>
        </div>
      </section>
    </div>

    <div class="mesh-strip">
      <div
        v-for="p in providers"
        :key="p.name"
        class="mesh-node"
        :class="[p.running ? 'on' : 'off', p.health_status === 'degraded' ? 'warn' : '']"
        :title="`${p.name} · ${p.health_status}`"
      >
        <HugeiconsIcon :icon="p.running ? ServerStack01Icon : GlobeIcon" :size="14" aria-hidden="true" />
        <span class="mesh-name">{{ p.name }}</span>
        <span class="mesh-metric">{{ p.running ? `${p.latency_ms ?? 0}ms` : t('common.offline') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hub-panel {
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
  font-family: var(--font-arcade);
  font-size: 0.5rem;
  color: var(--orange);
  letter-spacing: 0.12em;
  text-transform: uppercase;
}
.panel-title {
  font-family: var(--font-arcade);
  font-size: 0.92rem;
  line-height: 1.5;
  margin: 0.55rem 0 0.45rem;
  color: var(--text);
}
.panel-copy {
  color: var(--muted-strong);
  font-size: 0.76rem;
  line-height: 1.55;
  max-width: 46ch;
}

.hub-state {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  padding: 0.55rem 0.75rem;
  background: var(--void-2);
}
.hub-state .state-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--danger);
  box-shadow: 0 0 8px var(--danger);
}
.hub-state.online .state-dot {
  background: var(--cyan);
  box-shadow: 0 0 10px var(--cyan);
  animation: pulse-dot 1.4s ease-in-out infinite;
}
@keyframes pulse-dot {
  50% { opacity: 0.35; }
}
.hub-state-text {
  display: flex;
  flex-direction: column;
  gap: 0.1rem;
}
.hub-state-text strong {
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text);
}
.hub-state-text span {
  font-size: 0.64rem;
  color: var(--muted);
}

.runtime-row {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.55rem 0.8rem;
  border-radius: var(--radius-sm);
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  border: 1px solid var(--line);
}
.runtime-row.ok {
  color: var(--cyan);
  border-color: rgba(92, 225, 230, 0.35);
  background: var(--cyan-soft);
}
.runtime-row.bad {
  color: #ffc1c8;
  border-color: rgba(255, 91, 110, 0.4);
  background: var(--danger-soft);
}
.runtime-uptime {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  color: var(--muted-strong);
  font-family: var(--font-mono);
}

.stat-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 0.7rem;
}
.stat-tile {
  padding: 0.8rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
.stat-tile-head {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  color: var(--orange);
}
.accent-cyan .stat-tile-head {
  color: var(--cyan);
}
.stat-label {
  font-size: 0.6rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--muted-strong);
}
.stat-value-row {
  display: flex;
  align-items: baseline;
  gap: 0.4rem;
}
.stat-value {
  font-family: var(--font-arcade);
  font-size: 1rem;
  color: var(--text);
}
.accent-orange .stat-value {
  color: var(--orange);
  text-shadow: var(--glow-orange);
}
.accent-cyan .stat-value {
  color: var(--cyan);
  text-shadow: var(--glow-cyan);
}
.stat-unit {
  font-size: 0.6rem;
  color: var(--muted);
  text-transform: uppercase;
}

.hub-lower {
  display: grid;
  grid-template-columns: 1.4fr 1fr;
  gap: 0.7rem;
}
.chart-card,
.gauge-card {
  padding: 0.9rem;
}
.chart-card-head {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  font-size: 0.64rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--muted-strong);
  margin-bottom: 0.7rem;
}
.chart-card-head svg {
  color: var(--orange);
}
.chart-card-peak {
  margin-left: auto;
  color: var(--cyan);
}

.gauges {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}
.gauge-top {
  display: flex;
  justify-content: space-between;
  margin-bottom: 0.35rem;
}
.gauge-label {
  font-size: 0.64rem;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}
.gauge-value {
  font-family: var(--font-arcade);
  font-size: 0.66rem;
  color: var(--text);
}
.gauge-track {
  height: 9px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid var(--line);
  overflow: hidden;
}
.gauge-fill {
  height: 100%;
  transition: width 0.5s cubic-bezier(0.22, 1, 0.36, 1);
  box-shadow: 0 0 10px rgba(255, 149, 0, 0.4);
}

.mesh-strip {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr));
  gap: 0.5rem;
}
.mesh-node {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.5rem 0.6rem;
  border: 1px solid var(--line);
  border-radius: var(--radius-sm);
  background: var(--void-2);
  font-size: 0.64rem;
}
.mesh-node.on {
  border-color: rgba(92, 225, 230, 0.3);
  color: var(--cyan);
}
.mesh-node.on svg {
  filter: drop-shadow(0 0 4px rgba(92, 225, 230, 0.6));
}
.mesh-node.off {
  color: var(--muted);
  opacity: 0.7;
}
.mesh-node.warn {
  border-color: rgba(255, 149, 0, 0.5);
  color: var(--orange);
}
.mesh-name {
  text-transform: uppercase;
  letter-spacing: 0.05em;
  font-weight: 600;
}
.mesh-metric {
  margin-left: auto;
  color: var(--muted-strong);
}

@media (max-width: 900px) {
  .stat-grid {
    grid-template-columns: repeat(2, 1fr);
  }
  .hub-lower {
    grid-template-columns: 1fr;
  }
}
@media (max-width: 520px) {
  .stat-grid {
    grid-template-columns: 1fr;
  }
}
</style>
