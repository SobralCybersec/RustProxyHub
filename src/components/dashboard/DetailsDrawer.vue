<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'
import { storeToRefs } from 'pinia'
import { HugeiconsIcon } from '@hugeicons/vue'
import { Cancel01Icon, ServerStack01Icon } from '@hugeicons/core-free-icons'
import { useStore } from '@/store'
import type { ProviderOverview } from '@/lib/types'

const store = useStore()
const { activeDrawer, activeProviderDetails, activeProviderLogs } = storeToRefs(store)

// Mirror the pattern from ProviderGrid — returns the dot modifier only
function dotMod(overview: ProviderOverview): string {
  if (overview.last_error) return 'err'
  if (overview.health_status === 'healthy') return 'ok'
  if (overview.health_status === 'degraded' || overview.health_status === 'booting') return 'warn'
  return ''
}

function onEscape(e: KeyboardEvent) {
  if (e.key === 'Escape') store.closeProviderDrawer()
}

onMounted(() => document.addEventListener('keydown', onEscape))
onBeforeUnmount(() => document.removeEventListener('keydown', onEscape))
</script>

<template>
  <Transition name="drawer">
    <div v-if="activeDrawer" class="drawer-scrim" @click.self="store.closeProviderDrawer()">
      <div class="drawer" role="dialog" :aria-label="activeDrawer" aria-modal="true">
        <!-- Header -->
        <div class="row" style="margin-bottom: var(--md)">
          <HugeiconsIcon :icon="ServerStack01Icon" :size="16" class="muted" aria-hidden="true" />
          <span class="strong" style="font-size: 15px; text-transform: capitalize">
            {{ activeDrawer }}
          </span>
          <div class="spacer" />
          <button
            type="button"
            class="btn btn-icon"
            :aria-label="store.t('common.close')"
            @click="store.closeProviderDrawer()"
          >
            <HugeiconsIcon :icon="Cancel01Icon" :size="18" aria-hidden="true" />
          </button>
        </div>

        <!-- Loading skeleton -->
        <div
          v-if="!activeProviderDetails"
          class="faint"
          style="padding: var(--lg) 0; text-align: center; font-size: 14px"
        >
          Loading…
        </div>

        <!-- Content -->
        <div v-else class="stack">
          <!-- Overview card -->
          <div class="card stack" style="gap: var(--sm)">
            <!-- Status + login_state -->
            <div class="row" style="flex-wrap: wrap; gap: var(--xs)">
              <span :class="['dot', dotMod(activeProviderDetails.overview)]" />
              <span class="muted" style="font-size: 13px">
                {{ activeProviderDetails.overview.health_status }}
              </span>
              <div class="spacer" />
              <span class="chip">{{ activeProviderDetails.overview.login_state }}</span>
            </div>

            <!-- Base URL -->
            <div class="row" style="align-items: flex-start; gap: var(--xs)">
              <span class="faint" style="font-size: 12px; white-space: nowrap; padding-top: 1px">URL</span>
              <span class="mono" style="word-break: break-all">
                {{ activeProviderDetails.overview.base_url || store.t('providers.noBaseUrl') }}
              </span>
            </div>

            <!-- Web search chip -->
            <div v-if="activeProviderDetails.overview.web_search_supported" class="row">
              <span class="chip">web search</span>
            </div>

            <!-- Last error -->
            <p
              v-if="activeProviderDetails.overview.last_error"
              style="margin: 0; color: var(--err); font-size: 13px; word-break: break-word"
            >
              {{ activeProviderDetails.overview.last_error }}
            </p>
          </div>

          <!-- Models -->
          <div class="card">
            <div class="eyebrow" style="margin-bottom: var(--xs)">
              {{ store.t('providers.fields.models') }}
            </div>
            <div class="row" style="flex-wrap: wrap; gap: var(--xs)">
              <template v-if="activeProviderDetails.overview.models.length">
                <span v-for="model in activeProviderDetails.overview.models" :key="model" class="chip">
                  {{ model }}
                </span>
              </template>
              <span v-else class="faint" style="font-size: 13px">
                {{ store.t('providers.modelBufferEmpty') }}
              </span>
            </div>
          </div>

          <!-- Detail JSON (non-null only) -->
          <div v-if="activeProviderDetails.detail" class="card">
            <div class="eyebrow" style="margin-bottom: var(--xs)">Detail</div>
            <!-- ponytail: plain pre instead of a code component; upgrade if syntax highlighting is ever needed -->
            <pre class="log-frame">{{ JSON.stringify(activeProviderDetails.detail, null, 2) }}</pre>
          </div>

          <!-- Logs -->
          <div class="card">
            <div class="eyebrow" style="margin-bottom: var(--xs)">Logs</div>
            <pre v-if="activeProviderLogs.length" class="log-frame">{{ activeProviderLogs.join('\n') }}</pre>
            <span v-else class="faint" style="font-size: 13px">{{ store.t('details.noLogs') }}</span>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* Slide-in from the right — under 200ms, reduced-motion handled globally in main.css */
.drawer-enter-active,
.drawer-leave-active {
  transition: opacity 0.15s ease;
}
.drawer-enter-active .drawer,
.drawer-leave-active .drawer {
  transition: transform 0.15s ease;
}
.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}
.drawer-enter-from .drawer,
.drawer-leave-to .drawer {
  transform: translateX(100%);
}
</style>
