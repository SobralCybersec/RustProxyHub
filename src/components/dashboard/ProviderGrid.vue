<script setup lang="ts">
import type { ProviderName, ProviderOverview } from '@/lib/types'
import { useStore } from '@/store'

defineProps<{ providers: ProviderOverview[] }>()
const store = useStore()
const providerTitles: Record<ProviderName, string> = {
  qwen: 'Qwen Account Bank',
  deepseek: 'DeepSeek Bridge',
  kimi: 'Kimi Bridge',
  chatgpt: 'ChatGPT Session',
  gemini: 'Gemini Session',
}
function loginOpen(provider: ProviderName) {
  return store.overview?.open_provider_login_sessions.includes(provider) ?? false
}
function statusTone(provider: ProviderOverview) {
  if (!provider.running) return 'idle'
  if (provider.login_state === 'authenticated') return 'ok'
  if (provider.health_status === 'degraded') return 'warn'
  return 'run'
}
function formatStarted(value: number | null) {
  return value ? new Date(value * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : 'n/a'
}
</script>

<template>
  <section class="terminal-panel providers-terminal">
    <div class="terminal-line"><span class="prompt">visitor@rustproxy:~$</span> tail -f providers.log --square</div>
    <div class="table-head"><span>provider</span><span>status</span><span>models</span><span>base url</span><span>actions</span></div>

    <article v-for="provider in providers" :key="provider.name" class="provider-row" :data-state="statusTone(provider)">
      <div class="provider-name">
        <strong>{{ provider.name }}</strong>
        <small>{{ providerTitles[provider.name] }}</small>
      </div>
      <div>
        <code>{{ provider.login_state.replaceAll('_', ' ') }}</code>
        <small>{{ provider.health_status }} · started {{ formatStarted(provider.started_at) }}</small>
      </div>
      <div class="model-count">{{ provider.model_count }}</div>
      <div class="url-cell">{{ provider.base_url || 'no base url' }}</div>
      <div class="row-actions">
        <button :disabled="store.isBusy(`login:start:${provider.name}`)" @click.stop="store.startProviderLogin(provider.name)">
          {{ loginOpen(provider.name) ? 'reopen' : 'login' }}
        </button>
        <button :disabled="!loginOpen(provider.name)" @click.stop="store.stopProviderLogin(provider.name)">done</button>
        <button @click.stop="store.openProviderDrawer(provider.name)">dossier</button>
      </div>
      <div class="model-cloud">
        <span v-for="model in provider.models.slice(0, 12)" :key="`${provider.name}:${model}`">{{ provider.name }}:{{ model }}</span>
        <em v-if="!provider.models.length">empty model buffer</em>
        <em v-if="provider.models.length > 12">+{{ provider.models.length - 12 }} more</em>
      </div>
    </article>

    <div v-if="!providers.length" class="empty-terminal">stdout: no providers match the current grep filter.</div>
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

.providers-terminal { width: 100%; padding: .9rem; }
.terminal-line { color: #b9ffd0; font-size: 1.2rem; margin-bottom: .75rem; }
.prompt { color: #7fff9a; margin-right: .45rem; }
.table-head,
.provider-row {
  display: grid;
  grid-template-columns: 1.1fr 1.2fr .45fr 1.4fr 1fr;
  gap: .7rem;
  align-items: center;
}
.table-head {
  color: #7fff9a;
  text-transform: uppercase;
  letter-spacing: .13em;
  padding: .55rem .65rem;
  border: 1px solid rgba(144, 238, 144, .35);
  background: rgba(144, 238, 144, .08);
}
.provider-row {
  position: relative;
  padding: .75rem .65rem;
  border-left: 1px solid rgba(144, 238, 144, .35);
  border-right: 1px solid rgba(144, 238, 144, .35);
  border-bottom: 1px solid rgba(144, 238, 144, .22);
  background: #020702;
}
.provider-row::before {
  content: '[RUN]';
  color: #8affaa;
}
.provider-row[data-state='idle']::before { content: '[OFF]'; color: #6f8b76; }
.provider-row[data-state='warn']::before { content: '[WRN]'; color: #ffd37d; }
.provider-row[data-state='ok']::before { content: '[ OK]'; color: #b9ffd0; }
.provider-name strong { display: block; color: #e8fff0; text-transform: uppercase; }
.provider-name small,
.provider-row small { display: block; color: rgba(190, 255, 203, .66); margin-top: .18rem; }
code { color: #a9ffd0; }
.model-count { color: #effff3; font-size: 1.35rem; }
.url-cell { color: rgba(196, 255, 202, .76); word-break: break-all; }
.row-actions { display: flex; flex-wrap: wrap; gap: .35rem; }
button {
  border: 1px solid rgba(144, 238, 144, .42);
  background: #041004;
  color: lightgreen;
  padding: .45rem .6rem;
  cursor: pointer;
  font: inherit;
}
button:hover { background: rgba(144, 238, 144, .16); }
button:disabled { opacity: .42; cursor: not-allowed; }
.model-cloud {
  grid-column: 1 / -1;
  display: flex;
  flex-wrap: wrap;
  gap: .35rem;
  padding-left: 3.5rem;
}
.model-cloud span,
.model-cloud em {
  border: 1px solid rgba(144, 238, 144, .24);
  padding: .18rem .45rem;
  color: #a5ffc2;
  background: #000;
  font-style: normal;
}
.empty-terminal { padding: 1rem; color: rgba(196, 255, 202, .75); }
@media (max-width: 960px) {
  .table-head { display: none; }
  .provider-row { grid-template-columns: 1fr; border: 1px solid rgba(144, 238, 144, .35); margin-bottom: .5rem; }
  .model-cloud { padding-left: 0; }
}
</style>
