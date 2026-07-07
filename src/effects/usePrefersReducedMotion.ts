import { onUnmounted, ref } from 'vue'

const QUERY = '(prefers-reduced-motion: reduce)'

/** Sync helper for imperative / non-setup code (effects, plain functions). SSR-safe. */
export function prefersReducedMotion(): boolean {
  if (typeof window === 'undefined') return false
  return window.matchMedia(QUERY).matches
}

/**
 * Reactive ref that tracks the OS reduced-motion preference.
 * SSR-safe; removes the MediaQueryList listener on component unmount.
 */
export function usePrefersReducedMotion() {
  const mq = typeof window !== 'undefined' ? window.matchMedia(QUERY) : null
  const reduced = ref(mq?.matches ?? false)
  const handler = (e: MediaQueryListEvent) => {
    reduced.value = e.matches
  }
  mq?.addEventListener('change', handler)
  onUnmounted(() => mq?.removeEventListener('change', handler))
  return reduced
}
