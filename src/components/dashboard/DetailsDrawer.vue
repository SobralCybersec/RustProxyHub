<script setup lang="ts">
import { computed } from 'vue'
import { useStore } from '@/store'

const store = useStore()
const details = computed(() => store.activeProviderDetails)
const logs = computed(() => store.activeProviderLogs)
function prettyJson(value: unknown) {
  return JSON.stringify(value, null, 2)
}
</script>

<template>
  <aside v-if="store.activeDrawer" class="drawer-backdrop" @click.self="store.closeProviderDrawer()">
    <section class="drawer-terminal">
      <div class="drawer-titlebar">
        <span>provider://{{ store.activeDrawer }}</span>
        <button @click.stop="store.closeProviderDrawer()">x close</button>
      </div>
      <div class="terminal-line"><span class="prompt">root@rustproxy:/dossier$</span> inspect {{ store.activeDrawer }} --json --logs</div>
      <div class="drawer-grid">
        <article>
          <h3>overview.json</h3>
          <pre>{{ prettyJson(details?.overview ?? {}) }}</pre>
        </article>
        <article>
          <h3>health.json</h3>
          <pre>{{ prettyJson(details?.detail ?? {}) }}</pre>
        </article>
        <article>
          <h3>runtime.log</h3>
          <pre>{{ logs.length ? logs.join('\n') : 'No local lifecycle logs yet.' }}</pre>
        </article>
        <article v-if="details?.qwen_accounts?.length">
          <h3>qwen-accounts.json</h3>
          <pre>{{ prettyJson(details.qwen_accounts) }}</pre>
        </article>
      </div>
    </section>
  </aside>
</template>

<style scoped>
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

.drawer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  background: rgba(0, 0, 0, .78);
  display: flex;
  justify-content: flex-end;
  padding: 1rem;
  color: lightgreen;
}
.drawer-terminal {
  width: min(58rem, 100%);
  height: 100%;
  border: 3px solid lightgreen;
  background: #000;
  box-shadow: 0 0 32px rgba(144, 238, 144, .18), inset 0 0 24px rgba(144, 238, 144, .07);
  overflow: auto;
}
.drawer-titlebar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: .65rem .8rem;
  border-bottom: 1px solid rgba(144, 238, 144, .38);
  background: rgba(144, 238, 144, .12);
}
.drawer-titlebar span { color: #dfffe4; letter-spacing: .12em; text-transform: uppercase; }
.terminal-line { padding: .8rem; color: #b9ffd0; font-size: 1.2rem; }
.prompt { color: #7fff9a; margin-right: .45rem; }
.drawer-grid { display: grid; gap: .65rem; padding: 0 .8rem .8rem; }
article { border: 1px solid rgba(144, 238, 144, .35); background: #020702; overflow: hidden; }
h3 { margin: 0; padding: .55rem .65rem; border-bottom: 1px solid rgba(144, 238, 144, .24); color: #7fff9a; font-weight: 400; letter-spacing: .08em; }
pre { margin: 0; padding: .65rem; max-height: 22rem; overflow: auto; color: rgba(196, 255, 202, .82); white-space: pre-wrap; word-break: break-word; }
button { border: 1px solid rgba(144, 238, 144, .42); background: #041004; color: lightgreen; padding: .45rem .65rem; cursor: pointer; font: inherit; }
button:hover { background: rgba(144, 238, 144, .16); }
</style>
