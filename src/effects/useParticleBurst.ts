/**
 * useParticleBurst — fires a short-lived pixel-spark burst at a screen point.
 * A single shared full-screen canvas is created on first use; particles are
 * pooled per-burst and the loop self-terminates when none remain, so there is
 * zero idle cost when nothing is firing.
 */

import { prefersReducedMotion } from './usePrefersReducedMotion'

interface Particle {
  x: number
  y: number
  vx: number
  vy: number
  life: number
  maxLife: number
  size: number
  color: string
}

let canvas: HTMLCanvasElement | null = null
let context: CanvasRenderingContext2D | null = null
const particles: Particle[] = []
let running = false
let rafHandle = 0
let resizeCleanup: (() => void) | null = null

function ensureCanvas() {
  if (canvas) return
  canvas = document.createElement('canvas')
  canvas.style.cssText =
    'position:fixed;inset:0;width:100%;height:100%;pointer-events:none;z-index:90;'
  canvas.setAttribute('aria-hidden', 'true')
  document.body.appendChild(canvas)
  context = canvas.getContext('2d')
  const resize = () => {
    if (!canvas) return
    canvas.width = window.innerWidth
    canvas.height = window.innerHeight
  }
  resize()
  window.addEventListener('resize', resize)
  resizeCleanup = () => window.removeEventListener('resize', resize)
}

function loop() {
  if (!context || !canvas) return
  running = true
  context.clearRect(0, 0, canvas.width, canvas.height)
  for (let i = particles.length - 1; i >= 0; i -= 1) {
    const p = particles[i]
    p.life -= 1
    if (p.life <= 0) {
      particles.splice(i, 1)
      continue
    }
    p.vy += 0.12
    p.x += p.vx
    p.y += p.vy
    const alpha = p.life / p.maxLife
    context.globalAlpha = alpha
    context.fillStyle = p.color
    context.fillRect(p.x, p.y, p.size, p.size)
  }
  context.globalAlpha = 1
  if (particles.length > 0) {
    rafHandle = window.requestAnimationFrame(loop)
  } else {
    running = false
    rafHandle = 0
  }
}

export function particleBurst(x: number, y: number, color = '#ff9500', amount = 16) {
  if (typeof document === 'undefined') return
  if (prefersReducedMotion()) return
  ensureCanvas()
  const palette = [color, '#ffb347', '#5ce1e6']
  for (let i = 0; i < amount; i += 1) {
    const angle = (Math.PI * 2 * i) / amount + Math.random() * 0.5
    const speed = 1.6 + Math.random() * 3.4
    particles.push({
      x,
      y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed - 1,
      life: 26 + Math.random() * 20,
      maxLife: 46,
      size: 2 + Math.floor(Math.random() * 2),
      color: palette[Math.floor(Math.random() * palette.length)],
    })
  }
  if (!running) rafHandle = window.requestAnimationFrame(loop)
}

/** Convenience: burst from the center of an element (e.g. a button click). */
export function burstFromElement(el: Element, color?: string, amount?: number) {
  const rect = el.getBoundingClientRect()
  particleBurst(rect.left + rect.width / 2, rect.top + rect.height / 2, color, amount)
}

/** Tear down the shared canvas and cancel any in-flight animation. Safe to call on unmount. */
export function disposeParticleBurst() {
  if (rafHandle) window.cancelAnimationFrame(rafHandle)
  rafHandle = 0
  running = false
  particles.length = 0
  resizeCleanup?.()
  resizeCleanup = null
  if (canvas) {
    canvas.remove()
    canvas = null
    context = null
  }
}
