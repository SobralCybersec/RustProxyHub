<script setup lang="ts">
import { HugeiconsIcon } from '@hugeicons/vue'
import {
  BrowserIcon,
  Cancel01Icon,
  Globe02Icon,
  HelpCircleIcon,
  Maximize01Icon,
  Minimize01Icon,
  Settings02Icon,
  SparklesIcon,
  VolumeHighIcon,
  VolumeOffIcon,
} from '@hugeicons/core-free-icons'
import { getCurrentWindow } from '@tauri-apps/api/window'
import gsap from 'gsap'
import { storeToRefs } from 'pinia'
import * as THREE from 'three'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import DetailsDrawer from '@/components/dashboard/DetailsDrawer.vue'
import HubHeader from '@/components/dashboard/HubHeader.vue'
import LoginStudio from '@/components/dashboard/LoginStudio.vue'
import ProviderGrid from '@/components/dashboard/ProviderGrid.vue'
import QwenAccountsPanel from '@/components/dashboard/QwenAccountsPanel.vue'
import WorkbenchPanel from '@/components/dashboard/WorkbenchPanel.vue'
import { localeTimeCode, messageAt, type UiLocale } from '@/lib/ui-i18n'
import { useStore } from '@/store'
import { bindInteractionSounds, closeAudio, muted as audioMuted, playTone, toggleMute } from '@/effects/useArcadeAudio'
import { disposeParticleBurst, usePrefersReducedMotion } from '@/effects'

type ConsoleTab = 'overview' | 'providers' | 'access' | 'qwen' | 'workbench'

const store = useStore()
const { overview, error, notice, filteredProviders } = storeToRefs(store)
const t = (key: string, params?: Record<string, string | number>) => store.t(key, params)

const activeTab = ref<ConsoleTab>('overview')
const navMenuOpen = ref(false)
const typedTitle = ref('')
const metricCredits = ref(0)
const metricConnections = ref(0)
const metricActiveProxies = ref(0)
const copiedAccessLink = ref(false)
const backgroundCanvas = ref<HTMLCanvasElement | null>(null)
const introHexCanvas = ref<HTMLCanvasElement | null>(null)

const introVisible = ref(true)
const introExiting = ref(false)
const introFlash = ref(false)
const introProgress = ref(0)
const introBootVisible = ref(0)
const introGlitch = ref(false)

const runtimeReady = computed(() => overview.value?.runtime.single_runner_ready ?? false)
const hubRunning = computed(() => overview.value?.hub.running ?? false)
const runningProviders = computed(() => filteredProviders.value.filter((provider) => provider.running))
const displayedHubUrl = computed(() => overview.value?.hub.base_url ?? 'hub offline')
const accessLink = computed(() => overview.value?.hub.openapi_url ?? `${displayedHubUrl.value}/docs`)
const localeCode = computed(() => localeTimeCode(store.locale))
const titleText = computed(() => t('app.titleText'))
const introLines = computed(() => messageAt(store.locale, 'app.intro.bootLines') as string[])
const introTagline = computed(() => t('app.intro.tagline'))

const headerTabs = computed<Array<{ key: ConsoleTab; label: string }>>(() => [
  { key: 'overview', label: t('app.headerTabs.overview') },
  { key: 'providers', label: t('app.headerTabs.providers') },
  { key: 'access', label: t('app.headerTabs.access') },
  { key: 'qwen', label: t('app.headerTabs.qwen') },
  { key: 'workbench', label: t('app.headerTabs.workbench') },
])

const panelTabs = computed<Array<{ key: ConsoleTab; label: string }>>(() => [
  { key: 'overview', label: t('app.panelTabs.overview') },
  { key: 'providers', label: t('app.panelTabs.providers') },
  { key: 'access', label: t('app.panelTabs.access') },
  { key: 'qwen', label: t('app.panelTabs.qwen') },
  { key: 'workbench', label: t('app.panelTabs.workbench') },
])

const localeOptions: Array<{ key: UiLocale; label: string }> = [
  { key: 'pt-BR', label: 'PT' },
  { key: 'en', label: 'EN' },
]

const clock = ref(new Date())
let clockTimer: number | null = null
let backgroundCleanup: (() => void) | null = null
let introCleanup: (() => void) | null = null
let interactionCleanup: (() => void) | null = null
let actionEffectsCleanup: (() => void) | null = null
let typeTimer: number | null = null

const creditTarget = computed(() => (overview.value?.hub.model_count ?? 0) * 80 + store.qwenAccounts.length * 40)
const connectionTarget = computed(() => store.openLoginCount)
const activeProxyTarget = computed(() => runningProviders.value.length)
const activityCells = computed(() =>
  Array.from({ length: 52 }, (_, index) => {
    const provider = filteredProviders.value[index % Math.max(filteredProviders.value.length, 1)]
    if (!provider) return 0
    const seed = provider.model_count + index + (provider.running ? 4 : 1)
    return Math.min(4, seed % 5)
  }),
)
const latencySeries = computed(() => {
  const series = overview.value?.latency_series
  if (series && series.length >= 7) return series.slice(-12)
  return [42, 38, 45, 32, 28, 35, 29]
})
const statusLine = computed(() => {
  const status = hubRunning.value ? t('app.statusHubOnline') : t('app.statusHubPending')
  const avg = latencySeries.value.reduce((sum, value) => sum + value, 0) / latencySeries.value.length
  return t('app.statusLine', { status, avg: Math.round(avg), credits: creditTarget.value })
})
const titleKicker = computed(() => (runtimeReady.value ? t('app.runtimeReady') : t('app.runtimePending')))
const reducedMotion = usePrefersReducedMotion()

function formatMetric(value: number) {
  return Math.floor(value).toLocaleString(localeCode.value)
}

function animateCounter(targetRef: typeof metricCredits, value: number, duration: number, delay = 0) {
  if (reducedMotion.value) {
    targetRef.value = value
    return
  }
  const source = { value: targetRef.value }
  gsap.to(source, {
    value,
    duration: duration / 1000,
    delay: delay / 1000,
    ease: 'expo.out',
    onUpdate: () => {
      targetRef.value = Math.round(source.value)
    },
  })
}

function tickTitle(text = titleText.value, index = 0) {
  if (reducedMotion.value) {
    typedTitle.value = text
    return
  }
  if (typeTimer) window.clearTimeout(typeTimer)
  typedTitle.value = text.slice(0, index)
  if (index > text.length) return
  typeTimer = window.setTimeout(() => tickTitle(text, index + 1), 36)
}

function copyAccessLink() {
  navigator.clipboard
    ?.writeText(accessLink.value)
    .then(() => {
      copiedAccessLink.value = true
      playTone('success')
      window.setTimeout(() => {
        copiedAccessLink.value = false
      }, 1800)
    })
    .catch(() => {
      copiedAccessLink.value = false
    })
}

function selectTab(tab: ConsoleTab) {
  activeTab.value = tab
  navMenuOpen.value = false
}

function setLocale(locale: UiLocale) {
  store.setLocale(locale)
}

function mountBackground() {
  if (reducedMotion.value) return
  const canvas = backgroundCanvas.value
  if (!canvas) return
  backgroundCleanup?.()

  const scene = new THREE.Scene()
  const camera = new THREE.PerspectiveCamera(60, window.innerWidth / window.innerHeight, 1, 1000)
  camera.position.z = 280

  const webglContext =
    canvas.getContext('webgl2', { alpha: true, antialias: true }) ??
    canvas.getContext('webgl', { alpha: true, antialias: true })
  if (!webglContext) return

  const renderer = new THREE.WebGLRenderer({
    canvas,
    context: webglContext as WebGLRenderingContext,
    alpha: true,
    antialias: true,
  })
  renderer.setSize(window.innerWidth, window.innerHeight)
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))

  const count = 130
  const positions = new Float32Array(count * 3)
  const velocities = Array.from({ length: count }, () => ({
    x: (Math.random() - 0.5) * 0.22,
    y: (Math.random() - 0.5) * 0.22,
    z: (Math.random() - 0.5) * 0.22,
  }))
  for (let index = 0; index < count; index += 1) {
    positions[index * 3] = (Math.random() - 0.5) * 700
    positions[index * 3 + 1] = (Math.random() - 0.5) * 400
    positions[index * 3 + 2] = (Math.random() - 0.5) * 300
  }

  const particleGeometry = new THREE.BufferGeometry()
  particleGeometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  const particleMaterial = new THREE.PointsMaterial({ color: 0xff9500, size: 2.2, transparent: true, opacity: 0.85 })
  const points = new THREE.Points(particleGeometry, particleMaterial)
  scene.add(points)

  const lineGeometry = new THREE.BufferGeometry()
  const linePositions = new Float32Array(count * count * 3)
  lineGeometry.setAttribute('position', new THREE.BufferAttribute(linePositions, 3))
  const lineMaterial = new THREE.LineBasicMaterial({ color: 0xff9500, transparent: true, opacity: 0.07 })
  const lines = new THREE.LineSegments(lineGeometry, lineMaterial)
  scene.add(lines)

  let frame = 0
  let disposed = false
  const resize = () => {
    camera.aspect = window.innerWidth / window.innerHeight
    camera.updateProjectionMatrix()
    renderer.setSize(window.innerWidth, window.innerHeight)
  }
  const animate = () => {
    if (disposed) return
    frame = window.requestAnimationFrame(animate)
    const pos = particleGeometry.attributes.position.array as Float32Array
    let lineIndex = 0
    for (let a = 0; a < count; a += 1) {
      pos[a * 3] += velocities[a].x
      pos[a * 3 + 1] += velocities[a].y
      pos[a * 3 + 2] += velocities[a].z
      if (Math.abs(pos[a * 3]) > 350) velocities[a].x *= -1
      if (Math.abs(pos[a * 3 + 1]) > 200) velocities[a].y *= -1
      if (Math.abs(pos[a * 3 + 2]) > 150) velocities[a].z *= -1
      for (let b = a + 1; b < count; b += 1) {
        const dx = pos[a * 3] - pos[b * 3]
        const dy = pos[a * 3 + 1] - pos[b * 3 + 1]
        const dz = pos[a * 3 + 2] - pos[b * 3 + 2]
        const distance = Math.sqrt(dx * dx + dy * dy + dz * dz)
        if (distance < 88) {
          linePositions[lineIndex++] = pos[a * 3]
          linePositions[lineIndex++] = pos[a * 3 + 1]
          linePositions[lineIndex++] = pos[a * 3 + 2]
          linePositions[lineIndex++] = pos[b * 3]
          linePositions[lineIndex++] = pos[b * 3 + 1]
          linePositions[lineIndex++] = pos[b * 3 + 2]
        }
      }
    }
    lineGeometry.setDrawRange(0, lineIndex / 3)
    lineGeometry.attributes.position.needsUpdate = true
    particleGeometry.attributes.position.needsUpdate = true
    points.rotation.y += 0.0004
    lines.rotation.y += 0.0004
    renderer.render(scene, camera)
  }
  window.addEventListener('resize', resize)
  animate()

  backgroundCleanup = () => {
    disposed = true
    window.cancelAnimationFrame(frame)
    window.removeEventListener('resize', resize)
    renderer.dispose()
    particleGeometry.dispose()
    particleMaterial.dispose()
    lineGeometry.dispose()
    lineMaterial.dispose()
  }
}

function animateCards() {
  if (!reducedMotion.value) {
    gsap.fromTo(
      '.card-enter',
      { y: 20, opacity: 0 },
      {
        y: 0,
        opacity: 1,
        duration: 0.7,
        delay: 0.14,
        ease: 'power2.out',
        stagger: { each: 0.07, from: 'start' },
      },
    )
  }

  actionEffectsCleanup?.()
  const cards = document.querySelectorAll<HTMLElement>('.card-hover')
  if (!reducedMotion.value) {
    cards.forEach((card) => {
      card.onmouseenter = () => gsap.to(card, { scale: 1.012, y: -2, duration: 0.18, ease: 'power1.out' })
      card.onmouseleave = () => gsap.to(card, { scale: 1, y: 0, duration: 0.18, ease: 'power1.out' })
    })
  }
  const cleanups = Array.from(cards, (card) => {
    const reset = () => {
      card.style.setProperty('--spotlight-x', '50%')
      card.style.setProperty('--spotlight-y', '50%')
      card.style.setProperty('--card-rotate-x', '0deg')
      card.style.setProperty('--card-rotate-y', '0deg')
      card.style.setProperty('--card-shift-x', '0px')
      card.style.setProperty('--card-shift-y', '0px')
    }
    const move = (event: PointerEvent) => {
      const rect = card.getBoundingClientRect()
      if (!rect.width || !rect.height) return
      const px = (event.clientX - rect.left) / rect.width
      const py = (event.clientY - rect.top) / rect.height
      card.style.setProperty('--spotlight-x', `${Math.max(0, Math.min(1, px)) * 100}%`)
      card.style.setProperty('--spotlight-y', `${Math.max(0, Math.min(1, py)) * 100}%`)
      card.style.setProperty('--card-rotate-y', `${(px - 0.5) * 10}deg`)
      card.style.setProperty('--card-rotate-x', `${(0.5 - py) * 8}deg`)
      card.style.setProperty('--card-shift-x', `${(px - 0.5) * 8}px`)
      card.style.setProperty('--card-shift-y', `${(py - 0.5) * 6}px`)
    }
    reset()
    card.addEventListener('pointermove', move)
    card.addEventListener('pointerleave', reset)
    return () => {
      card.removeEventListener('pointermove', move)
      card.removeEventListener('pointerleave', reset)
      reset()
    }
  })
  actionEffectsCleanup = () => {
    cleanups.forEach((cleanup) => cleanup())
    actionEffectsCleanup = null
  }
}


function mountIntroRain() {
  if (reducedMotion.value) return
  const canvas = introHexCanvas.value
  if (!canvas) return
  const context = canvas.getContext('2d')
  if (!context) return
  const glyphs = 'RUSTPROXY0123456789ABCDEF'
  const columns: number[] = []
  let disposed = false
  let frame = 0
  const resize = () => {
    canvas.width = window.innerWidth
    canvas.height = window.innerHeight
    const totalColumns = Math.ceil(canvas.width / 16)
    columns.length = totalColumns
    for (let index = 0; index < totalColumns; index += 1) {
      columns[index] = Math.random() * canvas.height * 0.8
    }
  }
  const draw = () => {
    if (disposed) return
    frame = window.requestAnimationFrame(draw)
    context.fillStyle = 'rgba(0,0,0,0.12)'
    context.fillRect(0, 0, canvas.width, canvas.height)
    context.fillStyle = 'rgba(255,149,0,0.26)'
    context.font = '12px "JetBrains Mono", monospace'
    columns.forEach((y, index) => {
      const glyph = glyphs[Math.floor(Math.random() * glyphs.length)]
      context.fillText(glyph, index * 16, y)
      columns[index] = y > canvas.height + Math.random() * 200 ? 0 : y + 9 + Math.random() * 5
    })
  }
  resize()
  window.addEventListener('resize', resize)
  draw()
  introCleanup = () => {
    disposed = true
    window.cancelAnimationFrame(frame)
    window.removeEventListener('resize', resize)
  }
}

function delay(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

async function animateIntroProgress(from: number, to: number, durationMs: number) {
  return new Promise<void>((resolve) => {
    const start = performance.now()
    const range = to - from
    const tick = (now: number) => {
      const ratio = Math.min((now - start) / durationMs, 1)
      const eased = ratio < 0.5 ? 2 * ratio * ratio : -1 + (4 - 2 * ratio) * ratio
      introProgress.value = Math.round(from + range * eased)
      if (ratio < 1) window.requestAnimationFrame(tick)
      else resolve()
    }
    window.requestAnimationFrame(tick)
  })
}

async function glitchBurst() {
  for (let index = 0; index < 6; index += 1) {
    introGlitch.value = index % 2 === 0
    await delay(48)
  }
  introGlitch.value = false
}

function exitIntro(force = false) {
  if (!introVisible.value) return
  introCleanup?.()
  if (force) {
    introVisible.value = false
    introExiting.value = false
    introFlash.value = false
    return
  }
  introExiting.value = true
  window.setTimeout(() => {
    introVisible.value = false
    introExiting.value = false
  }, 720)
}

async function runIntroSequence(initPromise: Promise<unknown>) {
  if (reducedMotion.value) {
    introBootVisible.value = introLines.value.length
    introProgress.value = 100
    await initPromise.catch(() => undefined)
    exitIntro(true)
    return
  }
  introBootVisible.value = 0
  introProgress.value = 0
  introGlitch.value = false
  for (let index = 0; index < introLines.value.length; index += 1) {
    await delay(index === 0 ? 200 : 240)
    introBootVisible.value = index + 1
    playTone('tap')
  }
  await delay(220)
  await Promise.all([animateIntroProgress(0, 65, 900), delay(260)])
  await glitchBurst()
  await animateIntroProgress(65, 100, 620)
  await initPromise.catch(() => undefined)
  await delay(160)
  introFlash.value = true
  playTone('accent')
  await delay(120)
  introFlash.value = false
  await delay(180)
  exitIntro()
}

function initVisuals() {
  try {
    mountBackground()
    animateCards()
    animateCounter(metricCredits, creditTarget.value, 1800, 240)
    animateCounter(metricConnections, connectionTarget.value, 1500, 420)
    animateCounter(metricActiveProxies, activeProxyTarget.value, 1400, 560)
  } catch {
    metricCredits.value = creditTarget.value
    metricConnections.value = connectionTarget.value
    metricActiveProxies.value = activeProxyTarget.value
  }
}

watch([creditTarget, connectionTarget, activeProxyTarget], () => {
  animateCounter(metricCredits, creditTarget.value, 1100)
  animateCounter(metricConnections, connectionTarget.value, 950)
  animateCounter(metricActiveProxies, activeProxyTarget.value, 950)
})

watch(titleText, () => {
  tickTitle(titleText.value)
})

watch(
  () => store.locale,
  () => {
    tickTitle(titleText.value)
  },
)

onMounted(async () => {
  const initPromise = store.initApp()
  clockTimer = window.setInterval(() => {
    clock.value = new Date()
  }, 1000)
  tickTitle(titleText.value)
  interactionCleanup = bindInteractionSounds()
  mountIntroRain()
  void nextTick(() => {
    initVisuals()
  })
  void runIntroSequence(initPromise)
  await initPromise
})

onBeforeUnmount(() => {
  store.disposeApp()
  if (clockTimer) window.clearInterval(clockTimer)
  if (typeTimer) window.clearTimeout(typeTimer)
  backgroundCleanup?.()
  introCleanup?.()
  interactionCleanup?.()
  actionEffectsCleanup?.()
  disposeParticleBurst()
  closeAudio()
})
</script>

<template>
  <div class="app-shell" :data-locale="store.locale">
    <div class="crt-overlay" aria-hidden="true" />
    <div class="crt-flicker" aria-hidden="true" />

    <div v-if="introVisible" id="intro-overlay" :class="{ exit: introExiting, flash: introFlash }">
      <canvas id="hex-canvas" ref="introHexCanvas" aria-hidden="true" />
      <div id="boot-log">
        <div
          v-for="(line, index) in introLines"
          :key="line"
          class="log-line"
          :class="{ visible: index < introBootVisible }"
        >
          {{ line }}
        </div>
      </div>
      <div id="intro-center">
        <div id="intro-logo">
          RUST<span class="accent">PROXY</span>HUB
          <div class="glitch-copy" :class="{ visible: introGlitch }" aria-hidden="true">RUSTPROXYHUB</div>
        </div>
        <div id="intro-tagline">{{ introTagline }}</div>
        <div id="intro-bar-wrap"><div id="intro-bar-fill" :style="{ width: `${introProgress}%` }" /></div>
        <div id="intro-bar-pct">{{ introProgress }}%</div>
      </div>
      <button id="intro-skip" type="button" @click="exitIntro(true)">{{ t('app.intro.skip') }}</button>
    </div>

    <div id="page-content" :class="{ visible: !introVisible }">
      <canvas id="bg" ref="backgroundCanvas" aria-hidden="true" />

      <header class="navbar">
        <div class="nav-left">
          <div class="logo">RUST<span>PROXY</span>HUB</div>
          <nav class="nav-links" :class="{ active: navMenuOpen }" aria-label="Main sections">
            <button
              v-for="tab in headerTabs"
              :key="tab.key"
              type="button"
              class="nav-link"
              :class="{ active: activeTab === tab.key }"
              @click="selectTab(tab.key)"
            >
              {{ tab.label }}
            </button>
          </nav>
        </div>

        <div class="nav-right">
          <div class="locale-switch" :aria-label="t('app.localeLabel')">
            <button
              v-for="option in localeOptions"
              :key="option.key"
              type="button"
              class="locale-button"
              :class="{ active: store.locale === option.key }"
              @click="setLocale(option.key)"
            >
              <HugeiconsIcon v-if="option.key === 'en'" :icon="Globe02Icon" :size="14" aria-hidden="true" />
              <span>{{ option.label }}</span>
            </button>
          </div>
          <div class="window-badge" :class="{ live: runtimeReady }">
            <HugeiconsIcon :icon="BrowserIcon" :size="16" aria-hidden="true" />
            <span>{{ titleKicker }}</span>
          </div>
          <div class="window-actions">
            <button type="button" class="chrome-button" :aria-label="audioMuted ? 'Unmute sound effects' : 'Mute sound effects'" @click="toggleMute()">
              <HugeiconsIcon :icon="audioMuted ? VolumeOffIcon : VolumeHighIcon" :size="15" aria-hidden="true" />
            </button>
            <button type="button" class="chrome-button" :aria-label="t('app.actions.help')" @click="selectTab('workbench')">
              <HugeiconsIcon :icon="HelpCircleIcon" :size="15" aria-hidden="true" />
            </button>
            <button type="button" class="chrome-button" :aria-label="t('app.actions.minimize')" @click="getCurrentWindow().minimize()">
              <HugeiconsIcon :icon="Minimize01Icon" :size="15" aria-hidden="true" />
            </button>
            <button type="button" class="chrome-button" :aria-label="t('app.actions.maximize')" @click="getCurrentWindow().toggleMaximize()">
              <HugeiconsIcon :icon="Maximize01Icon" :size="15" aria-hidden="true" />
            </button>
            <button type="button" class="chrome-button danger" :aria-label="t('app.actions.close')" @click="getCurrentWindow().close()">
              <HugeiconsIcon :icon="Cancel01Icon" :size="15" aria-hidden="true" />
            </button>
            <button type="button" class="hamburger" :aria-label="t('app.actions.openMenu')" @click="navMenuOpen = !navMenuOpen">
              ≡
            </button>
          </div>
        </div>
      </header>

      <div class="container">
        <main class="main-column">
          <div class="status-bar">
            <span>{{ statusLine }}</span>
            <span>
              {{ clock.toLocaleTimeString(localeCode, { hour: '2-digit', minute: '2-digit', second: '2-digit' }) }}
              · {{ displayedHubUrl }}
            </span>
          </div>

          <h1 class="main-title">{{ typedTitle }}</h1>

          <div v-if="notice" class="info-banner">{{ notice }}</div>
          <div v-if="error" class="error-banner">{{ error }}</div>

          <div class="actions">
            <button
              type="button"
              class="card card-action card-hover card-enter"
              :disabled="hubRunning || store.isBusy('service:start:hub')"
              @click="store.startService('hub')"
            >
              <div class="card-icon">LIVE</div>
              <HugeiconsIcon :icon="SparklesIcon" :size="22" aria-hidden="true" />
              <h2>{{ hubRunning ? t('app.hubOnline') : t('app.startHub') }}</h2>
              <p>{{ hubRunning ? t('app.startHubCopyOnline') : t('app.startHubCopyOffline') }}</p>
            </button>

            <button type="button" class="card card-action card-hover card-enter" @click="store.refreshOverview()">
              <div class="card-icon">SYNC</div>
              <HugeiconsIcon :icon="Settings02Icon" :size="22" aria-hidden="true" />
              <h2>{{ t('app.refreshPanel') }}</h2>
              <p>{{ t('app.shellRefreshCopy') }}</p>
            </button>
          </div>

          <div class="tabs" role="tablist" aria-label="Panel tabs">
            <button
              v-for="tab in panelTabs"
              :id="`tab-${tab.key}`"
              :key="tab.key"
              type="button"
              role="tab"
              class="tab"
              :class="{ active: activeTab === tab.key }"
              :aria-selected="activeTab === tab.key"
              :aria-controls="`tabpanel-${tab.key}`"
              @click="selectTab(tab.key)"
            >
              {{ tab.label }}
            </button>
          </div>

          <section class="panel-shell">
            <div v-show="activeTab === 'overview'" id="tabpanel-overview" role="tabpanel" aria-labelledby="tab-overview"><HubHeader :overview="overview" /></div>
            <div v-show="activeTab === 'providers'" id="tabpanel-providers" role="tabpanel" aria-labelledby="tab-providers"><ProviderGrid :providers="filteredProviders" /></div>
            <div v-show="activeTab === 'access'" id="tabpanel-access" role="tabpanel" aria-labelledby="tab-access"><LoginStudio :providers="filteredProviders" /></div>
            <div v-show="activeTab === 'qwen'" id="tabpanel-qwen" role="tabpanel" aria-labelledby="tab-qwen"><QwenAccountsPanel /></div>
            <div v-show="activeTab === 'workbench'" id="tabpanel-workbench" role="tabpanel" aria-labelledby="tab-workbench"><WorkbenchPanel /></div>
          </section>
        </main>

        <aside class="sidebar">
          <div class="sidebar-section">
            <div class="card sidebar-card metrics-card card-enter">
              <div class="sidebar-kicker">{{ t('app.metrics.activeProxies') }}</div>
              <div class="metrics">
                <div class="metric">
                  <div class="metric-label">{{ t('app.metrics.credits') }}</div>
                  <div class="metric-value">{{ formatMetric(metricCredits) }}</div>
                </div>
                <div class="metric">
                  <div class="metric-label">{{ t('app.metrics.connections') }}</div>
                  <div class="metric-value">{{ formatMetric(metricConnections) }}</div>
                </div>
                <div class="metric">
                  <div class="metric-label">{{ t('app.metrics.activeProxies') }}</div>
                  <div class="metric-value">{{ formatMetric(metricActiveProxies) }}</div>
                </div>
              </div>
              <button type="button" class="btn-comprar" @click="selectTab('providers')">
                {{ t('app.sidebar.openProviderManager') }}
              </button>
            </div>

            <div class="card sidebar-card signal-card card-enter">
              <div class="activity-header">
                <span class="activity-title">{{ t('app.sidebar.activity') }}</span>
                <span class="activity-count">52 WK</span>
              </div>
              <div class="heatmap">
                <span v-for="(cell, index) in activityCells" :key="index" class="heat-cell" :class="`heat-${cell}`" />
              </div>
              <div class="chart-label">LATENCY MS</div>
              <div class="chart-container">
                <SparkChart :values="latencySeries" mode="line" />
              </div>
            </div>
          </div>

          <div class="sidebar-section">
            <div class="card sidebar-card optimize-card card-enter">
              <div class="sidebar-kicker">{{ t('app.panelTabs.workbench') }}</div>
              <p>
                <strong>{{ t('app.sidebar.optimizeTitle') }}</strong><br />
                {{ t('app.sidebar.optimizeCopy') }}
              </p>
            </div>

            <div class="card sidebar-card access-card card-enter">
              <div class="referral-title">{{ t('app.sidebar.accessTitle') }}</div>
              <div class="referral-box">
                <input class="referral-input" :value="accessLink" readonly />
                <button type="button" class="btn-copy" :class="{ copied: copiedAccessLink }" @click="copyAccessLink">
                  {{ copiedAccessLink ? t('common.copied') : t('common.copy') }}
                </button>
              </div>
            </div>
          </div>
        </aside>
      </div>

      <DetailsDrawer />
    </div>
  </div>
</template>
