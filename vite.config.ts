import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import vueJsx from '@vitejs/plugin-vue-jsx'
import vueDevTools from 'vite-plugin-vue-devtools'

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  // componentInspector 必须关掉：它会把 <script setup lang="tsx"> 的脚本块用 isTSX:true 去解析，
  // 于是 defineProps<{...}>() / defineModel<T>() 这类泛型调用会被当成 JSX 标签，直接 500（章节详情页就是这么挂的）。
  // 关掉只影响「点组件在编辑器里打开」，devtools 面板照旧。
  plugins: [vue(), UnoCSS(), vueJsx(), vueDevTools({ componentInspector: false })],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 5005,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
}))
