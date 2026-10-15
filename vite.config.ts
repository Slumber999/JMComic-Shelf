import { defineConfig, transformWithEsbuild } from 'vite'
import type { Plugin } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import vueJsx from '@vitejs/plugin-vue-jsx'
import vueDevTools from 'vite-plugin-vue-devtools'

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST

/// <script setup lang="tsx"> 的脚本子请求（id 形如 `<file>.vue?vue&type=script&lang.tsx`）。
/// vueJsx 的 include 是 /\.[jt]sx$/，这个 id 正好以 `lang.tsx` 结尾，于是整块脚本被它
/// 当成独立 TSX 文件交给 babel（isTSX:true）解析：`PropType<() => void>` 这种带箭头函数
/// 的类型参数会被当成 JSX 标签，模块直接 500（详情页的章节面板就是这么挂的）。
function isVueTsxScript(id: string): boolean {
  const queryIndex = id.indexOf('?')
  if (queryIndex === -1) {
    return false
  }
  const query = new URLSearchParams(id.slice(queryIndex + 1))
  // vue 插件拼的是无值参数 lang.tsx（不是 lang=tsx），两种写法都要认，否则这个插件永远不生效
  return (
    query.has('vue') &&
    query.get('type') === 'script' &&
    (query.get('lang') === 'tsx' || query.has('lang.tsx'))
  )
}

/// 先用 esbuild 把这块脚本的类型去掉、保留 JSX：之后 vueJsx 只处理 JSX，不会再误解析类型参数。
/// 只作用于 .vue 的 tsx 脚本块，独立的 .tsx 文件（如 ChapterCheckbox.tsx）完全不受影响。
function vueTsxScriptPlugin(): Plugin {
  return {
    name: 'jmcomic:vue-tsx-script',
    enforce: 'pre',
    async transform(code, id) {
      if (!isVueTsxScript(id)) {
        return
      }
      return transformWithEsbuild(code, id, { loader: 'tsx', jsx: 'preserve' })
    },
  }
}

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  // componentInspector 必须关掉：它会把 <script setup lang="tsx"> 的脚本块用 isTSX:true 去解析，
  // 于是 defineProps<{...}>() / defineModel<T>() 这类泛型调用会被当成 JSX 标签，直接 500（章节详情页就是这么挂的）。
  // 关掉只影响「点组件在编辑器里打开」，devtools 面板照旧。
  plugins: [vueTsxScriptPlugin(), vue(), UnoCSS(), vueJsx(), vueDevTools({ componentInspector: false })],

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
