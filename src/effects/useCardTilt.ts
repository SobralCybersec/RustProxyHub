// vTilt directive: subtle 3D pointer-tilt on cards. Registered globally in main.ts.
// ponytail: re-implemented minimal after the original (which also had unused bindCardTilt) was
// deleted; expand the easing curve if a steeper tilt is needed.
import type { Directive } from 'vue'

interface TiltState {
  cleanup: () => void
}

const MAX_TILT = 6 // degrees

export const vTilt: Directive<HTMLElement, never> = {
  mounted(el) {
    el.style.transformStyle = 'preserve-3d'
    el.style.transition = 'transform 0.18s ease-out'

    const onMove = (event: PointerEvent) => {
      const rect = el.getBoundingClientRect()
      const px = (event.clientX - rect.left) / rect.width - 0.5
      const py = (event.clientY - rect.top) / rect.height - 0.5
      el.style.transform = `perspective(800px) rotateX(${(-py * MAX_TILT).toFixed(2)}deg) rotateY(${(px * MAX_TILT).toFixed(2)}deg)`
    }
    const onLeave = () => {
      el.style.transform = 'perspective(800px) rotateX(0deg) rotateY(0deg)'
    }

    el.addEventListener('pointermove', onMove)
    el.addEventListener('pointerleave', onLeave)

    ;(el as unknown as { __tilt?: TiltState }).__tilt = {
      cleanup: () => {
        el.removeEventListener('pointermove', onMove)
        el.removeEventListener('pointerleave', onLeave)
      },
    }
  },
  unmounted(el) {
    ;(el as unknown as { __tilt?: TiltState }).__tilt?.cleanup()
  },
}