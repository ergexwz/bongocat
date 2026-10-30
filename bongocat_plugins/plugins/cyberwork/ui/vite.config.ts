import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  base: './',
  plugins: [vue()],
  build: {
    target: 'chrome105',
    outDir: 'dist',
    emptyOutDir: true,
  },
})
