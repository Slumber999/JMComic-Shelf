<script setup lang="ts">
import { ref } from 'vue'
import { PhX } from '@phosphor-icons/vue'
import { useI18n } from '../../i18n.ts'
import { useCommentKeywords } from '../../commentBlock.ts'

/// 评论屏蔽设置：底部抽屉里增删关键词，直接走 commentBlock 的共享列表，改动立即同步到评论页和抽屉
const showing = defineModel<boolean>('showing', { required: true })

const { t } = useI18n()
const { keywords, addKeyword, removeKeyword } = useCommentKeywords()
const input = ref('')

function submit(): void {
  if (input.value.trim() === '') {
    return
  }
  addKeyword(input.value)
  input.value = ''
}
</script>

<template>
  <teleport to="body">
    <div v-if="showing" class="backdrop" @click="showing = false">
      <section class="sheet" @click.stop>
        <header class="sheet-head">
          <span class="sheet-title">{{ t('comments.block.title') }}</span>
          <button class="close" type="button" :aria-label="t('comments.block.close')" @click="showing = false">
            <ph-x :size="18" />
          </button>
        </header>

        <form class="add-row" @submit.prevent="submit">
          <input
            v-model="input"
            class="input"
            type="text"
            enterkeyhint="done"
            :placeholder="t('comments.block.placeholder')" />
          <button class="add" type="submit" :disabled="input.trim() === ''">
            {{ t('comments.block.add') }}
          </button>
        </form>

        <p v-if="keywords.length === 0" class="empty">{{ t('comments.block.empty') }}</p>
        <ul v-else class="keywords">
          <li v-for="keyword in keywords" :key="keyword" class="keyword">
            <span class="keyword-text">{{ keyword }}</span>
            <button
              class="remove"
              type="button"
              :aria-label="t('comments.block.remove')"
              @click="removeKeyword(keyword)">
              <ph-x :size="14" />
            </button>
          </li>
        </ul>

        <p class="hint">{{ t('comments.block.hint') }}</p>
      </section>
    </div>
  </teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: flex;
  align-items: flex-end;
  background: rgb(0 0 0 / 40%);
}

.sheet {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 100%;
  max-height: 72vh;
  overflow-y: auto;
  padding: 14px 16px calc(16px + env(safe-area-inset-bottom));
  border-radius: 16px 16px 0 0;
  background: #fff;
  box-sizing: border-box;
}

.sheet-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.sheet-title {
  font-size: 15px;
  font-weight: 600;
}

.close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: #f4f4f4;
  color: #444;
}

.add-row {
  display: flex;
  gap: 8px;
}

.input {
  flex: 1;
  min-width: 0;
  height: 40px;
  padding: 0 12px;
  border: 1px solid #e6e6e6;
  border-radius: 10px;
  background: #fff;
  color: inherit;
  font-size: 14px;
  outline: none;
}

.input:focus {
  border-color: #ffb152;
}

.add {
  flex: 0 0 auto;
  padding: 0 16px;
  border: none;
  border-radius: 10px;
  background: #ff7a00;
  color: #fff;
  font-size: 14px;
}

.add:disabled {
  opacity: 0.5;
}

.empty,
.hint {
  margin: 0;
  color: #888;
  font-size: 12px;
  line-height: 1.5;
}

.keywords {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 40vh;
  overflow-y: auto;
  margin: 0;
  padding: 0;
  list-style: none;
}

.keyword {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 8px 8px 12px;
  border-radius: 10px;
  background: #f7f7f7;
}

.keyword-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.remove {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 50%;
  background: #fff;
  color: #d03050;
}
</style>
