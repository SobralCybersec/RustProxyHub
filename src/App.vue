<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import { Cancel01Icon, Globe02Icon, Maximize01Icon, Minimize01Icon } from '@hugeicons/core-free-icons'
import { storeToRefs } from 'pinia'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'
import { useStore } from '@/store'
import type { UiLocale } from '@/lib/ui-i18n'

type Tab = 'overview' | 'providers' | 'access' | 'workbench'

const store = useStore()
const { error, notice } = storeToRefs(store)
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const activeTab = ref<Tab>('overview')

const tabs = computed<Array<{ key: Tab; label: string }>>(() => [
  { key: 'overview', label: t('app.headerTabs.overview') },
  { key: 'providers', label: t('app.headerTabs.providers') },
  { key: 'access', label: t('app.headerTabs.access') },
  { key: 'workbench', label: t('app.headerTabs.workbench') },
])

const locales: Array<{ key: UiLocale; label: string }> = [
  { key: 'pt-BR', label: 'PT' },
  { key: 'en', label: 'EN' },
]

// Window controls only work inside a Tauri window (decorations:false → we own the chrome).
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

async function windowAction(action: 'minimize' | 'toggleMaximize' | 'close') {
  if (!inTauri) return
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  const win = getCurrentWindow()
  if (action === 'minimize') await win.minimize()
  else if (action === 'toggleMaximize') await win.toggleMaximize()
  else await win.close()
}

onMounted(() => store.initApp())
onBeforeUnmount(() => store.disposeApp())
</script>

<template>
  <div class="app">
    <!-- Global nav: black, frameless-window drag region + window controls -->
    <header class="global-nav" data-tauri-drag-region>
      <span class="brand">RustProxyHub</span>
      <span class="spacer" data-tauri-drag-region></span>

      <div class="locale">
        <HugeiconsIcon :icon="Globe02Icon" :size="14" />
        <button
          v-for="loc in locales"
          :key="loc.key"
          class="locale-btn"
          :class="{ active: store.locale === loc.key }"
          type="button"
          @click="store.setLocale(loc.key)"
        >
          {{ loc.label }}
        </button>
      </div>

      <div v-if="inTauri" class="win-controls">
        <button class="win-btn" type="button" aria-label="Minimize" @click="windowAction('minimize')">
          <HugeiconsIcon :icon="Minimize01Icon" :size="14" />
        </button>
        <button class="win-btn" type="button" aria-label="Maximize" @click="windowAction('toggleMaximize')">
          <HugeiconsIcon :icon="Maximize01Icon" :size="14" />
        </button>
        <button class="win-btn close" type="button" aria-label="Close" @click="windowAction('close')">
          <HugeiconsIcon :icon="Cancel01Icon" :size="14" />
        </button>
      </div>
    </header>

    <!-- Sub-nav: frosted tab strip -->
    <nav class="sub-nav">
      <h3 class="sub-title">{{ tabs.find(tab => tab.key === activeTab)?.label }}</h3>
      <span class="spacer"></span>
      <button
        v-for="tab in tabs"
        :key="tab.key"
        class="tab"
        :class="{ active: activeTab === tab.key }"
        type="button"
        @click="activeTab = tab.key"
      >
        {{ tab.label }}
      </button>
    </nav>

    <main class="container">
      <div v-if="error" class="banner banner-error">{{ error }}</div>
      <div v-if="notice" class="banner banner-notice">{{ notice }}</div>

      <HubHeader v-if="activeTab === 'overview'" />
      <ProviderGrid v-else-if="activeTab === 'providers'" />
      <LoginStudio v-else-if="activeTab === 'access'" />
      <WorkbenchPanel v-else-if="activeTab === 'workbench'" />
    </main>

    <DetailsDrawer />
  </div>
</template>

<style scoped>
.app {
  min-height: 100vh;
}

.brand {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: -0.224px;
}

.locale {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--text-muted);
}
.locale-btn {
  background: transparent;
  border: none;
  color: var(--text-faint);
  font-size: 12px;
  padding: 4px 6px;
  border-radius: var(--r-sm);
  transition:
    color 0.15s ease,
    transform 0.1s ease;
}
.locale-btn:active {
  transform: scale(0.95);
}
.locale-btn.active {
  color: var(--text);
}

.win-controls {
  display: flex;
  gap: 2px;
  margin-left: var(--sm);
}
.win-btn {
  width: 30px;
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  border-radius: var(--r-sm);
  color: var(--text-muted);
}
.win-btn:hover {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text);
}
.win-btn.close:hover {
  background: var(--err);
  color: #fff;
}

.sub-title {
  font-size: 21px;
  font-weight: 600;
  letter-spacing: -0.224px;
}
</style>
