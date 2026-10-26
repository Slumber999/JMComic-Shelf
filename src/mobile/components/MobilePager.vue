<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { NButton, NModal } from 'naive-ui'
import { PhCaretLeft, PhCaretRight } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'

/// 窄屏分页器：只保留「上一页 / 第 n / N 页 / 下一页」。
/// 中间点开的是页数窄列表：默认显示当前页附近 ±20 页，滚到顶/底自动续接，点一项直接跳。
const props = defineProps<{ page: number; pageCount: number }>()
const emit = defineEmits<{ 'update:page': [page: number] }>()
const { t } = useI18n()

const JUMP_SPAN = 20
const ITEM_HEIGHT = 36

const jumpOpen = ref(false)
const rangeStart = ref(1)
const rangeEnd = ref(1)
const listEl = ref<HTMLElement>()

const isFirst = computed(() => props.page <= 1)
const isLast = computed(() => props.page >= props.pageCount)

/// 当前窗口内的页码，一项一页
const pages = computed(() => {
  const result: number[] = []
  for (let page = rangeStart.value; page <= rangeEnd.value; page += 1) {
    result.push(page)
  }
  return result
})

function goTo(page: number) {
  if (page === props.page) {
    return
  }
  emit('update:page', page)
}

function openJump() {
  rangeStart.value = Math.max(1, props.page - JUMP_SPAN)
  rangeEnd.value = Math.min(props.pageCount, props.page + JUMP_SPAN)
  jumpOpen.value = true
  // 打开时把当前页滚到列表中间，附近的页一眼可见
  void nextTick(() => {
    const el = listEl.value
    if (el === undefined) {
      return
    }
    el.scrollTop = Math.max(0, (props.page - rangeStart.value) * ITEM_HEIGHT - (el.clientHeight - ITEM_HEIGHT) / 2)
  })
}

function choose(page: number) {
  jumpOpen.value = false
  goTo(page)
}

/// 滚到两端就再补 20 页；补完把滚动位置推回去，视线不跳
let expanding = false

function onListScroll() {
  if (expanding) {
    return
  }
  const el = listEl.value
  if (el === undefined) {
    return
  }

  if (el.scrollTop <= 0 && rangeStart.value > 1) {
    const add = Math.min(JUMP_SPAN, rangeStart.value - 1)
    expanding = true
    rangeStart.value -= add
    requestAnimationFrame(() => {
      el.scrollTop += add * ITEM_HEIGHT
      expanding = false
    })
    return
  }

  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 1 && rangeEnd.value < props.pageCount) {
    expanding = true
    rangeEnd.value = Math.min(props.pageCount, rangeEnd.value + JUMP_SPAN)
    requestAnimationFrame(() => {
      expanding = false
    })
  }
}
</script>

<template>
  <div v-if="pageCount > 1" class="pager">
    <n-button class="pager-side" :disabled="isFirst" tertiary @click="goTo(page - 1)">
      <template #icon>
        <ph-caret-left />
      </template>
      {{ t('mobile.pagerPrev') }}
    </n-button>

    <button class="pager-current" type="button" @click="openJump">
      {{ t('mobile.pagerOf', { current: page, total: pageCount }) }}
    </button>

    <n-button class="pager-side" :disabled="isLast" tertiary @click="goTo(page + 1)">
      {{ t('mobile.pagerNext') }}
      <template #icon>
        <ph-caret-right />
      </template>
    </n-button>

    <!-- 弹窗被 teleport 到 body，scoped 宽度选不中，宽度只能用内联 style -->
    <n-modal
      v-model:show="jumpOpen"
      preset="card"
      class="jump-card"
      :style="{ width: 'min(220px, 66vw)' }"
      :title="t('mobile.pagerJumpTitle')">
      <div ref="listEl" class="jump-list" @scroll.passive="onListScroll">
        <button
          v-for="item in pages"
          :key="item"
          type="button"
          class="jump-item"
          :class="{ active: item === page }"
          @click="choose(item)">
          {{ item }}
        </button>
      </div>
    </n-modal>
  </div>
</template>

<style scoped>
.pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
  /* 下面还有屏幕容器的 84px：这里再留 48px，上一页/下一页不会被 FAB 压住 */
  padding: 12px 0 48px;
}

.pager-side {
  height: 40px;
  padding: 0 12px;
  border-radius: 10px;
  font-size: 14px;
}

.pager-current {
  min-width: 118px;
  height: 40px;
  padding: 0 12px;
  border: 1px solid #e2e2e2;
  border-radius: 10px;
  background: #fff;
  color: #333;
  font-size: 14px;
  font-weight: 600;
}

.pager-current:active {
  background: #f4f4f4;
}

/* 窄列表：高度受限、可滚动，点某项直接跳页 */
.jump-list {
  max-height: min(52vh, 320px);
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  scrollbar-width: none;
}

.jump-list::-webkit-scrollbar {
  display: none;
}

.jump-item {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  min-height: 36px;
  padding: 0 8px;
  border: none;
  border-radius: 8px;
  background: none;
  color: #333;
  font-size: 14px;
}

.jump-item:active {
  background: #f4f4f4;
}

.jump-item.active {
  background: #fff2e6;
  color: #d96200;
  font-weight: 700;
}
</style>
