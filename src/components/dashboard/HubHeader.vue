<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import { Alert02Icon, ServerStack01Icon } from '@hugeicons/core-free-icons'
import { computed } from 'vue'
import { storeToRefs } from 'pinia'
import { useStore } from '@/store'

const store = useStore()
const { overview, runtimeReady, runtimeIssues, openLoginCount } = storeToRefs(store)

// ponytail: filter runs on every render tick — acceptable at ≤8 providers; lift to computed if list grows large.
const runningProviders = computed(() => overview.value?.providers.filter(p => p.running).length ?? 0)
</script>

<template>
  <!-- Loading guard -->
  <div v-if="!overview" class="loading-shell">
    <p class="muted">{{ store.t('hubHeader.loading') }}</p>
  </div>

  <div v-else class="stack">
    <!-- 1. Hero -->
    <div class="hero">
      <span class="eyebrow">
        {{ runtimeReady ? store.t('app.runtimeReady') : store.t('app.runtimePending') }}
      </span>
      <h1>RustProxyHub</h1>
      <p class="lead">{{ store.t('hubHeader.tagline') }}</p>
    </div>

    <!-- 2a. Runtime issues banner (conditional) -->
    <div v-if="runtimeIssues.length" class="banner banner-error">
      <div class="row">
        <HugeiconsIcon :icon="Alert02Icon" :size="18" aria-hidden="true" />
        <span>{{ store.t('hubHeader.runtimeIssues') }}</span>
      </div>
      <ul class="issue-list">
        <li v-for="issue in runtimeIssues" :key="issue">{{ issue }}</li>
      </ul>
    </div>

    <!-- 2b. Runtime diagnostic rows -->
    <div class="card diag-card">
      <div class="row">
        <span class="dot" :class="overview.runtime.browser_available ? 'ok' : 'err'" />
        <span class="muted">{{ store.t('hubHeader.browserAvailable') }}</span>
        <span class="spacer" />
        <span class="mono faint">{{
          overview.runtime.browser_available ? store.t('common.yes') : store.t('common.no')
        }}</span>
      </div>
      <div class="row">
        <span class="dot" :class="overview.runtime.single_runner_ready ? 'ok' : 'warn'" />
        <span class="muted">{{ store.t('hubHeader.singleRunner') }}</span>
        <span class="spacer" />
        <span class="mono faint">{{
          store.statusLabel(overview.runtime.single_runner_ready ? 'ready' : 'blocked')
        }}</span>
      </div>
    </div>

    <!-- 3. Hub status card -->
    <div class="card">
      <div class="row">
        <HugeiconsIcon :icon="ServerStack01Icon" :size="18" aria-hidden="true" />
        <span class="dot" :class="overview.hub.running ? 'ok' : ''" />
        <span class="strong">{{ overview.hub.running ? store.t('common.online') : store.t('common.offline') }}</span>
        <span class="spacer" />
        <button
          class="btn btn-primary"
          :disabled="overview.hub.running || store.isBusy('service:start:hub')"
          @click="store.startService('hub')"
        >
          {{ store.isBusy('service:start:hub') ? store.t('hubHeader.startingHub') : store.t('hubHeader.startHub') }}
        </button>
      </div>

      <div class="hub-fields">
        <div class="row">
          <span class="field-label">{{ store.t('hubHeader.baseUrl') }}</span>
          <span class="spacer" />
          <span class="mono muted">{{ overview.hub.base_url || '--' }}</span>
        </div>
        <div class="row">
          <span class="field-label">{{ store.t('hubHeader.port') }}</span>
          <span class="spacer" />
          <span class="mono muted">{{ overview.hub.port }}</span>
        </div>
        <div class="row">
          <span class="field-label">{{ store.t('hubHeader.apiKey') }}</span>
          <span class="spacer" />
          <span class="chip">
            {{ overview.hub.api_key_enabled ? store.t('common.enabled') : store.t('common.disabled') }}
          </span>
        </div>
      </div>
    </div>

    <!-- 4. Stat tiles -->
    <div class="grid">
      <div class="tile stat-tile">
        <span class="stat-num strong">{{ runningProviders }}</span>
        <span class="faint">{{ store.t('hubHeader.runningProviders') }}</span>
      </div>
      <div class="tile stat-tile">
        <span class="stat-num strong">{{ store.hubModelOptions.length }}</span>
        <span class="faint">{{ store.t('hub.stats.models') }}</span>
      </div>
      <div class="tile stat-tile">
        <span class="stat-num strong">{{ openLoginCount }}</span>
        <span class="faint">{{ store.t('hubHeader.openSessions') }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Loading placeholder: centered, doesn't collapse to 0 height */
.loading-shell {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
}

/* Hero: tighter than the default .stack gap */
.hero {
  display: flex;
  flex-direction: column;
  gap: var(--xs);
}

/* Two diagnostic rows need a gap the .card padding alone won't provide */
.diag-card {
  display: flex;
  flex-direction: column;
  gap: var(--sm);
}

/* Hub fields section: separator + vertical rhythm */
.hub-fields {
  display: flex;
  flex-direction: column;
  gap: var(--xs);
  margin-top: var(--sm);
  padding-top: var(--sm);
  border-top: 1px solid var(--hairline);
}

/* .field-label has margin-bottom: 6px by default; flatten it inside flex rows */
.hub-fields .field-label {
  margin-bottom: 0;
}

/* Tile internals: number stacked over label */
.stat-tile {
  display: flex;
  flex-direction: column;
  gap: var(--xxs);
}

.stat-num {
  font-size: 22px;
  line-height: 1;
  letter-spacing: -0.6px;
}

/* Banner issue list: reset browser defaults */
.issue-list {
  margin: var(--xs) 0 0;
  padding-left: var(--md);
}
</style>
