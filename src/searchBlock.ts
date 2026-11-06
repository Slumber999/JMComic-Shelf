import { computed, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import { commands } from './bindings.ts'
import { useI18n } from './i18n.ts'
import { useStore } from './store.ts'

export interface BlockableComic {
  id: number
  name?: string
}

export interface TagGroup {
  title: string
  content: string[]
}

/// 列表屏蔽词：官方标签点选 + 手动添加，命中的漫画不展示；分组目录由调用方提供
export function useSearchBlock<T extends BlockableComic>(
  comics: () => T[],
  groups: () => TagGroup[],
) {
  const store = useStore()
  const { t } = useI18n()
  const message = useMessage()

  const words = computed(() => store.config?.blockedTags ?? [])
  const groupsList = computed(() => groups())
  const manual = computed(() => store.config?.manualBlockedTags ?? [])
  const input = ref('')
  const blockedIds = ref<Set<number>>(new Set())

  const isBlocked = (comic: T) =>
    blockedIds.value.has(comic.id) || words.value.some((word) => (comic.name ?? '').includes(word))

  const visible = computed(() => comics().filter((comic) => !isBlocked(comic)))

  let token = 0
  async function refresh() {
    const run = ++token
    const list = comics()
    if (words.value.length === 0 || list.length === 0) {
      blockedIds.value = new Set()
      return
    }
    const result = await commands.blockedComicIds(
      list.map((comic) => comic.id),
      words.value,
      24,
    )
    if (run !== token || result.status === 'error') {
      return
    }
    blockedIds.value = new Set(result.data)
    const hidden = list.filter(isBlocked).length
    if (hidden > 0) {
      message.info(
        t('search.blockedNotice', { words: words.value.slice(0, 3).join('、'), count: hidden }),
      )
    }
  }

  watch([comics, words], () => void refresh(), { immediate: true })

  function save() {
    if (store.config !== undefined) {
      void commands.saveConfig(store.config)
    }
  }

  function setWords(next: string[]) {
    if (store.config === undefined) {
      return
    }
    store.config.blockedTags = next
    save()
  }

  function setManual(next: string[]) {
    if (store.config === undefined) {
      return
    }
    store.config.manualBlockedTags = next
    save()
  }

  function toggle(word: string) {
    const next = new Set(words.value)
    if (next.has(word)) {
      next.delete(word)
    } else {
      next.add(word)
    }
    setWords([...next])
    if (!next.has(word) && manual.value.includes(word)) {
      setManual(manual.value.filter((item) => item !== word))
    }
  }

  /// 手动词只启停屏蔽，词本身留在记忆里
  function toggleManual(word: string) {
    const active = words.value.includes(word)
    setWords(active ? words.value.filter((item) => item !== word) : [...words.value, word])
  }

  function forgetManual() {
    setWords(words.value.filter((word) => !manual.value.includes(word)))
    setManual([])
  }

  function add() {
    const word = input.value.trim()
    if (word === '') {
      return
    }
    setWords([...new Set([...words.value, word])])
    setManual([...new Set([...manual.value, word])])
    input.value = ''
  }

  return {
    words,
    manual,
    groups: groupsList,
    input,
    isBlocked,
    visible,
    toggle,
    toggleManual,
    forgetManual,
    add,
  }
}
