<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { drawSparkline } from '@/effects'

const props = withDefaults(
  defineProps<{
    values: number[]
    stroke?: string
    fill?: string
    mode?: 'line' | 'bars'
    height?: number
    min?: number
    max?: number
  }>(),
  {
    stroke: '#ff9500',
    fill: 'rgba(255,149,0,0.16)',
    mode: 'line',
    height: 40,
    min: undefined,
    max: undefined,
  },
)

const canvas = ref<HTMLCanvasElement | null>(null)

function render() {
  if (!canvas.value) return
  drawSparkline(canvas.value, {
    values: props.values,
    stroke: props.stroke,
    fill: props.fill,
    mode: props.mode,
    min: props.min,
    max: props.max,
  })
}

onMounted(render)
watch(() => props.values, render, { deep: true })
watch(() => [props.stroke, props.mode], render)
</script>

<template>
  <div class="spark" :style="{ height: `${height}px` }">
    <canvas ref="canvas" aria-hidden="true" />
  </div>
</template>

<style scoped>
.spark {
  position: relative;
  width: 100%;
}
.spark canvas {
  width: 100%;
  height: 100%;
  display: block;
}
</style>
