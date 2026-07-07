/**
 * drawSparkline — paints a compact line/area or bar sparkline onto a canvas.
 * Cheap to call on every telemetry tick; no external chart library needed, so
 * dozens of provider cards can each show a live trend without GC pressure.
 */

export interface SparklineOptions {
  values: number[]
  stroke?: string
  fill?: string
  lineWidth?: number
  mode?: 'line' | 'bars'
  min?: number
  max?: number
}

export function drawSparkline(canvas: HTMLCanvasElement, options: SparklineOptions) {
  const context = canvas.getContext('2d')
  if (!context) return

  const dpr = Math.min(window.devicePixelRatio || 1, 2)
  const cssWidth = canvas.clientWidth || 120
  const cssHeight = canvas.clientHeight || 36
  if (canvas.width !== cssWidth * dpr || canvas.height !== cssHeight * dpr) {
    canvas.width = cssWidth * dpr
    canvas.height = cssHeight * dpr
  }
  context.setTransform(dpr, 0, 0, dpr, 0, 0)
  context.clearRect(0, 0, cssWidth, cssHeight)

  const values = options.values
  if (!values.length) return

  const stroke = options.stroke ?? '#ff9500'
  const fill = options.fill ?? 'rgba(255,149,0,0.16)'
  const lineWidth = options.lineWidth ?? 1.5
  const pad = 2
  const min = options.min ?? Math.min(...values)
  const max = options.max ?? Math.max(...values)
  const range = max - min || 1

  const x = (i: number) => pad + (i / Math.max(values.length - 1, 1)) * (cssWidth - pad * 2)
  const y = (v: number) => cssHeight - pad - ((v - min) / range) * (cssHeight - pad * 2)

  if (options.mode === 'bars') {
    const gap = 1.5
    const barWidth = (cssWidth - pad * 2) / values.length - gap
    context.fillStyle = stroke
    values.forEach((v, i) => {
      const bx = pad + i * (barWidth + gap)
      const by = y(v)
      context.fillRect(bx, by, Math.max(barWidth, 1), cssHeight - pad - by)
    })
    return
  }

  context.beginPath()
  values.forEach((v, i) => {
    const px = x(i)
    const py = y(v)
    if (i === 0) context.moveTo(px, py)
    else context.lineTo(px, py)
  })

  // Area fill under the curve.
  context.lineTo(x(values.length - 1), cssHeight - pad)
  context.lineTo(x(0), cssHeight - pad)
  context.closePath()
  context.fillStyle = fill
  context.fill()

  // Stroke the line on top.
  context.beginPath()
  values.forEach((v, i) => {
    const px = x(i)
    const py = y(v)
    if (i === 0) context.moveTo(px, py)
    else context.lineTo(px, py)
  })
  context.strokeStyle = stroke
  context.lineWidth = lineWidth
  context.lineJoin = 'round'
  context.stroke()

  // Glowing head dot.
  const lastX = x(values.length - 1)
  const lastY = y(values[values.length - 1])
  context.beginPath()
  context.arc(lastX, lastY, 1.8, 0, Math.PI * 2)
  context.fillStyle = stroke
  context.fill()
}
