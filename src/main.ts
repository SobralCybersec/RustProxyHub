import { createPinia } from 'pinia'
import { createApp } from 'vue'
import App from './App.vue'
import { vTilt } from './effects'
import './assets/main.css'

const app = createApp(App)
app.use(createPinia())
app.directive('tilt', vTilt)
app.mount('#app')
