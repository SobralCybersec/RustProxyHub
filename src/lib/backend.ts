import type { ProviderName } from '@/lib/types'
import { invoke as mockInvoke } from '@/lib/mock-backend'

/**
 * Canonical provider order. Lives here (not in mock-backend) so the production
 * path never imports the mock. The mock re-exports its own copy for dev seeds.
 */
export const providerOrder: ProviderName[] = ['qwen', 'deepseek', 'kimi', 'chatgpt', 'gemini', 'mistral', 'zai', 'meta']

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/**
 * Real `invoke` inside a Tauri window; the in-browser mock during `vite dev` so
 * the UI still renders without a backend. The previous build routed *everything*
 * through the mock even when packaged — that was the wiring bug.
 */
export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    const { invoke: tauriInvoke } = await import('@tauri-apps/api/core')
    return tauriInvoke<T>(command, args)
  }
  return mockInvoke<T>(command, args)
}
