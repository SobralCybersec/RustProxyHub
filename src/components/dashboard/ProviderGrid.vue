<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  ArrowRight01Icon,
  ChatGptIcon,
  Copy01Icon,
  DeepseekIcon,
  Globe02Icon,
  GoogleGeminiIcon,
  KimiAiIcon,
  MetaIcon,
  MistralIcon,
  PlayIcon,
  QwenIcon,
  RoboticIcon,
  Search01Icon,
} from '@hugeicons/core-free-icons'
import { storeToRefs } from 'pinia'
import type { ProviderName, ProviderOverview } from '@/lib/types'
import { useStore } from '@/store'

const store = useStore()
const { searchQuery, filteredProviders } = storeToRefs(store)

// Brand glyphs per provider (same set the previous grid shipped). zai has no
// dedicated hugeicon, so it falls back to the generic RoboticIcon.
const PROVIDER_ICONS: Partial<Record<ProviderName, typeof RoboticIcon>> = {
  chatgpt: ChatGptIcon,
  deepseek: DeepseekIcon,
  gemini: GoogleGeminiIcon,
  kimi: KimiAiIcon,
  meta: MetaIcon,
  mistral: MistralIcon,
  qwen: QwenIcon,
  zai: RoboticIcon,
}

function providerIcon(name: ProviderName) {
  return PROVIDER_ICONS[name] ?? RoboticIcon
}

function dotClass(p: ProviderOverview): string {
  if (p.last_error) return 'err'
  if (p.health_status === 'healthy') return 'ok'
  if (p.health_status === 'degraded' || p.health_status === 'booting') return 'warn'
  return ''
}
</script>

<template>
  <div class="stack">
    <!-- search -->
    <div class="search-row">
      <HugeiconsIcon :icon="Search01Icon" :size="16" class="search-icon" aria-hidden="true" />
      <input
        class="input search"
        type="search"
        :placeholder="store.t('hubHeader.filterPlaceholder')"
        :value="searchQuery"
        @input="store.setSearchQuery(($event.target as HTMLInputElement).value)"
      />
    </div>

    <!-- empty state -->
    <p v-if="filteredProviders.length === 0" class="faint empty-msg">
      {{ store.t('providers.empty') }}
    </p>

    <!-- provider grid -->
    <div v-else class="grid">
      <div v-for="p in filteredProviders" :key="p.name" class="card stack provider-card">
        <!-- brand avatar + name + status dot -->
        <div class="row">
          <span class="provider-avatar">
            <HugeiconsIcon :icon="providerIcon(p.name)" :size="18" aria-hidden="true" />
          </span>
          <span class="strong provider-name">{{ p.name }}</span>
          <span :class="['dot', dotClass(p)]" />
          <span class="faint small">{{ store.statusLabel(p.health_status) }}</span>
        </div>

        <!-- chips -->
        <div class="row chips-row">
          <span class="chip">{{ store.statusLabel(p.login_state) }}</span>
          <span v-if="p.web_search_supported" class="chip">
            <HugeiconsIcon :icon="Globe02Icon" :size="12" aria-hidden="true" />
            {{ store.t('providers.webSearch') }}
          </span>
          <span class="faint small">{{ p.model_count }} {{ store.t('providers.fields.models') }}</span>
        </div>

        <!-- last error -->
        <p v-if="p.last_error" class="faint small err-text">{{ p.last_error }}</p>

        <!-- actions -->
        <div class="row actions-row">
          <button
            v-if="!p.running"
            type="button"
            class="btn btn-primary"
            :disabled="store.isBusy(`service:start:${p.name}`)"
            @click="store.startService(p.name)"
          >
            <HugeiconsIcon :icon="PlayIcon" :size="16" aria-hidden="true" />
            {{ store.t('providers.actions.start') }}
          </button>

          <button type="button" class="btn btn-utility" @click="store.openProviderDrawer(p.name)">
            <HugeiconsIcon :icon="ArrowRight01Icon" :size="16" aria-hidden="true" />
            {{ store.t('providers.actions.dossier') }}
          </button>

          <span class="spacer" />

          <button
            type="button"
            class="btn btn-utility copy-btn"
            :title="store.t('providers.actions.claude')"
            @click="store.copyAgentSetup(p.name, 'claude')"
          >
            <HugeiconsIcon :icon="Copy01Icon" :size="14" aria-hidden="true" />
            Claude
          </button>

          <button
            type="button"
            class="btn btn-utility copy-btn"
            :title="store.t('providers.actions.kilo')"
            @click="store.copyAgentSetup(p.name, 'kilo')"
          >
            <HugeiconsIcon :icon="Copy01Icon" :size="14" aria-hidden="true" />
            Kilo
          </button>

          <button
            type="button"
            class="btn btn-utility copy-btn"
            :title="store.t('providers.actions.pi')"
            @click="store.copyAgentSetup(p.name, 'pi')"
          >
            <HugeiconsIcon :icon="Copy01Icon" :size="14" aria-hidden="true" />
            Pi
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.search-row {
  position: relative;
  max-width: 420px;
}
.search-icon {
  position: absolute;
  left: var(--md);
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-faint);
  pointer-events: none;
}
/* push text past the icon */
.search-row .input.search {
  padding-left: calc(var(--md) + 16px + var(--xs));
}

.empty-msg {
  text-align: center;
  padding: var(--xl) 0;
}

.provider-card {
  gap: var(--sm);
}

.provider-avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex: none;
  border-radius: var(--r-sm);
  background: rgba(255, 255, 255, 0.06);
  color: var(--text);
}

.provider-name {
  text-transform: capitalize;
}

.small {
  font-size: 12px;
}

.chips-row {
  flex-wrap: wrap;
}

.err-text {
  color: var(--err);
  font-size: 12px;
}

.actions-row {
  flex-wrap: wrap;
  margin-top: auto;
}

/* smaller padding so copy buttons don't dominate */
.copy-btn {
  padding: 6px 10px;
  font-size: 12px;
}
</style>
