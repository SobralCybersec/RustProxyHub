<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { onBeforeUnmount, onMounted } from 'vue'
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue'
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'

const store = useStore()
const { overview, error, filteredProviders } = storeToRefs(store)

onMounted(() => {
  void store.initApp()
})

onBeforeUnmount(() => {
  store.disposeApp()
})
</script>

<template>
  <div class="shell">
    <div class="ambient ambient-a" />
    <div class="ambient ambient-b" />

    <main class="frame">
      <HubHeader :overview="overview" />

      <p v-if="error" class="error-banner">{{ error }}</p>

      <section class="board">
        <div class="lane services-lane">
          <ProviderGrid :providers="filteredProviders" />
          <QwenAccountsPanel />
        </div>

        <aside class="lane side-lane">
          <WorkbenchPanel />
          <LoginStudio :providers="filteredProviders" />
        </aside>
      </section>
    </main>

    <DetailsDrawer />
  </div>
</template>
