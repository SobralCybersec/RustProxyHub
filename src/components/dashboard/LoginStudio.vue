<script setup lang="ts">
import type { ProviderName, ProviderOverview } from '@/lib/types'
import { useStore } from '@/store'

defineProps<{ providers: ProviderOverview[] }>()
const store = useStore()
const guides: Record<ProviderName, string[]> = {
  qwen: ['open default profile when cookies expire', 'use Qwen vault for account-specific profiles', 'mark done after state is saved'],
  deepseek: ['open chat.deepseek.com', 'finish sign-in and wait for chat box', 'mark done then rerun smoke probe'],
  kimi: ['open kimi.com', 'wait for session storage to settle', 'mark done when page is ready'],
  chatgpt: ['authenticate on chatgpt.com', 'mark done so headless path can resume', 'model ids appear after probe'],
  gemini: ['authenticate on gemini.google.com', 'mark done after browser state is saved', 'model ids appear after probe'],
}
function loginOpen(provider: ProviderName) {
  return store.overview?.open_provider_login_sessions.includes(provider) ?? false
}
</script>

<template>
  <section class="terminal-panel login-terminal">
    <div class="terminal-line"><span class="prompt">visitor@rustproxy:~$</span> sudo loginctl --interactive --square</div>
    <div class="login-grid">
      <article v-for="provider in providers" :key="provider.name" class="login-buffer" :class="{ active: loginOpen(provider.name) }">
        <div class="buffer-head">
          <strong>{{ provider.name }}</strong>
          <span>{{ loginOpen(provider.name) ? '[ window open ]' : '[ idle ]' }}</span>
        </div>
        <label>
          <span>browser</span>
          <select v-model="store.browserPrefs[provider.name]">
            <option value="msedge">msedge</option>
            <option value="chrome">chrome</option>
            <option value="chromium">chromium</option>
          </select>
        </label>
        <ol>
          <li v-for="step in guides[provider.name]" :key="step">{{ step }}</li>
        </ol>
        <div class="actions">
          <button :disabled="store.isBusy(`login:start:${provider.name}`)" @click.stop="store.startProviderLogin(provider.name)">
            {{ loginOpen(provider.name) ? 'reopen session' : 'open session' }}
          </button>
          <button :disabled="!loginOpen(provider.name)" @click.stop="store.stopProviderLogin(provider.name)">mark done</button>
        </div>
      </article>
    </div>
  </section>
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

input, textarea, select { user-select: text; }

.login-terminal { width: 100%; padding: .9rem; }
.terminal-line { color: #b9ffd0; font-size: 1.2rem; margin-bottom: .75rem; }
.prompt { color: #7fff9a; margin-right: .45rem; }
.login-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr)); gap: .7rem; }
.login-buffer {
  border: 1px solid rgba(144, 238, 144, .35);
  background: #020702;
  padding: .8rem;
  position: relative;
  overflow: hidden;
}
.login-buffer.active {
  border-color: lightgreen;
  background: rgba(144, 238, 144, .08);
}
.login-buffer.active::before {
  content: '';
  position: absolute;
  inset: 0 0 auto;
  height: 2px;
  background: linear-gradient(90deg, transparent, lightgreen, transparent);
  animation: scan 1.4s linear infinite;
}
@keyframes scan { from { transform: translateX(-100%); } to { transform: translateX(100%); } }
.buffer-head { display: flex; justify-content: space-between; gap: .7rem; margin-bottom: .75rem; }
.buffer-head strong { text-transform: uppercase; color: #eafff0; }
.buffer-head span { color: rgba(196, 255, 202, .72); }
label { display: grid; gap: .35rem; color: #7fff9a; text-transform: uppercase; letter-spacing: .1em; }
select {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #000;
  color: lightgreen;
  padding: .6rem;
  font: inherit;
}
ol { margin: .75rem 0; padding-left: 1.4rem; color: rgba(196, 255, 202, .78); line-height: 1.55; }
.actions { display: flex; flex-wrap: wrap; gap: .4rem; }
button {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #041004;
  color: lightgreen;
  padding: .5rem .65rem;
  cursor: pointer;
  font: inherit;
}
button:hover { background: rgba(144, 238, 144, .16); }
button:disabled { opacity: .42; cursor: not-allowed; }
</style>
