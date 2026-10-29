import { createApp } from 'vue'

import App from './App.vue'
import { i18n } from './i18n'

import './plugins/todo/handdrawn.css'
import './bubble/bubble.css'

createApp(App).use(i18n).mount('#app')
