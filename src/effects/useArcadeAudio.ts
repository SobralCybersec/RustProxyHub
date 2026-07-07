/**
 * useArcadeAudio — tiny WebAudio synth for retro UI blips.
 * Lazily creates a single shared AudioContext and plays short square-wave tones.
 * Respects prefers-reduced-motion and a persisted mute flag (localStorage).
 */

import { ref } from 'vue'
import { prefersReducedMotion } from './usePrefersReducedMotion'

export type ToneKind = 'tap' | 'success' | 'accent' | 'error' | 'boot'

const MUTE_KEY = 'rph.audio.muted'

let audioContext: AudioContext | null = null

export const muted = ref(
  typeof localStorage !== 'undefined' && localStorage.getItem(MUTE_KEY) === 'true',
)

export function toggleMute() {
  muted.value = !muted.value
  if (typeof localStorage !== 'undefined') localStorage.setItem(MUTE_KEY, String(muted.value))
}

function getAudioContext(): AudioContext | null {
  if (typeof window === 'undefined') return null
  const AudioCtor =
    window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
  if (!AudioCtor) return null
  if (!audioContext) audioContext = new AudioCtor()
  return audioContext
}

interface ToneProfile {
  start: number
  end: number
  duration: number
  volume: number
  type: OscillatorType
}

const profiles: Record<ToneKind, ToneProfile> = {
  tap: { start: 320, end: 460, duration: 0.07, volume: 0.026, type: 'square' },
  success: { start: 440, end: 760, duration: 0.12, volume: 0.04, type: 'square' },
  accent: { start: 220, end: 540, duration: 0.16, volume: 0.05, type: 'square' },
  error: { start: 200, end: 90, duration: 0.18, volume: 0.05, type: 'sawtooth' },
  boot: { start: 120, end: 380, duration: 0.22, volume: 0.045, type: 'triangle' },
}

export function setMuted(value: boolean) {
  muted.value = value
  if (typeof localStorage !== 'undefined') localStorage.setItem(MUTE_KEY, String(value))
}

export function isMuted() {
  return muted.value
}

export function playTone(kind: ToneKind = 'tap') {
  if (muted.value || prefersReducedMotion()) return
  const context = getAudioContext()
  if (!context) return
  if (context.state === 'suspended') void context.resume()

  const profile = profiles[kind]
  const oscillator = context.createOscillator()
  const gain = context.createGain()
  oscillator.connect(gain)
  gain.connect(context.destination)

  const now = context.currentTime
  oscillator.type = profile.type
  oscillator.frequency.setValueAtTime(profile.start, now)
  oscillator.frequency.exponentialRampToValueAtTime(Math.max(profile.end, 1), now + profile.duration)
  gain.gain.setValueAtTime(0.0001, now)
  gain.gain.exponentialRampToValueAtTime(profile.volume, now + 0.015)
  gain.gain.exponentialRampToValueAtTime(0.0001, now + profile.duration)
  oscillator.start(now)
  oscillator.stop(now + profile.duration + 0.02)
}

/** Bind a global delegated click listener that blips on interactive elements. */
export function bindInteractionSounds(): () => void {
  if (typeof document === 'undefined') return () => {}
  const handler = (event: Event) => {
    const target = event.target as Element | null
    if (!target) return
    if (target.closest('button, .tab, .nav-link, .quick-link, .locale-button, [data-blip]')) {
      playTone('tap')
    }
  }
  document.addEventListener('pointerdown', handler, true)
  return () => document.removeEventListener('pointerdown', handler, true)
}

export function closeAudio() {
  void audioContext?.close()
  audioContext = null
}
