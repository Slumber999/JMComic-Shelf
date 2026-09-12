<script setup lang="ts">
import { ref, watch } from 'vue'
import { useStore } from '../../../store.ts'
import { NCheckbox, NInput, NRadio, NRadioGroup, NTooltip, useMessage } from 'naive-ui'

const store = useStore()

const message = useMessage()

const dirFmt = ref<string>(store.config?.dirFmt ?? '')

watch([() => store.config?.apiDomainMode, () => store.config?.customApiDomain], () => {
  message.warning('切换线路后可能需要重新登录')
})
</script>

<template>
  <div v-if="store.config !== undefined" class="flex flex-col">
    <span class="font-bold mt-2">下载格式</span>
    <n-radio-group v-model:value="store.config.downloadFormat">
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Jpeg">jpg</n-radio>
        </template>
        1. 有损
        <span class="text-red">(肉眼看不出)</span>
        <br />
        2. 文件体积小
        <br />
        4. 宽高的上限为65534
        <span class="text-red">(某些条漫可能会超过这个上限导致报错)</span>
        <br />
        3. 编码速度最快
        <br />
      </n-tooltip>
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Png">png</n-radio>
        </template>
        1. 无损
        <br />
        2. 文件体积大
        <span class="text-red">(约为jpg的5倍)</span>
        <br />
        3. 编码速度最慢
        <br />
      </n-tooltip>
      <n-tooltip placement="top" trigger="hover">
        <template #trigger>
          <n-radio value="Webp">webp</n-radio>
        </template>
        1. 无损
        <br />
        2. 文件体积大
        <span class="text-red">(约为jpg的4倍)</span>
        <br />
        3. 宽高的上限为16383
        <span class="text-red">(某些条漫可能会超过这个上限导致报错)</span>
        <br />
        4. 编码速度较慢
        <br />
      </n-tooltip>
    </n-radio-group>

    <span class="font-bold mt-2">下载目录格式</span>
    <n-tooltip placement="top" trigger="hover" :width="550">
      <div>
        可以用斜杠
        <span class="rounded bg-gray-500 px-1 text-white">/</span>
        来分隔目录层级
      </div>
      <div class="text-orange">至少要有两个层级，最后一层存放章节元数据，倒数第二层存放漫画元数据</div>
      <div class="font-semibold mt-2">可用字段：</div>
      <div class="grid grid-cols-2">
        <div>
          <span class="rounded bg-gray-500 px-1">comic_id</span>
          <span class="ml-2">漫画ID</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">chapter_id</span>
          <span class="ml-2">章节ID</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">comic_title</span>
          <span class="ml-2">漫画标题</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">chapter_title</span>
          <span class="ml-2">章节标题</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">author</span>
          <span class="ml-2">作者</span>
        </div>
        <div>
          <span class="rounded bg-gray-500 px-1">order</span>
          <span class="ml-2">章节在漫画里对应的序号</span>
        </div>
      </div>
      <div class="font-semibold mt-2">例如格式</div>
      <div class="bg-gray-200 rounded-md p-1 text-black w-fit">
        {author}/[{author}] {comic_title}({comic_id})/{order} - {chapter_title}
      </div>
      <div class="font-semibold">下载《蓦然回首》第1话会产生三层文件夹，分别是</div>
      <div class="flex gap-1 text-black">
        <span class="bg-gray-200 rounded-md px-2 w-fit">藤本树, 藤本タツキ</span>
        <span class="rounded bg-gray-500 px-1 text-white">/</span>
        <span class="bg-gray-200 rounded-md px-2 w-fit">[藤本树, 藤本タツキ] 蓦然回首(384524)</span>
        <span class="rounded bg-gray-500 px-1 text-white">/</span>
        <span class="bg-gray-200 rounded-md px-2 w-fit">1 - 第1话</span>
      </div>
      <template #trigger>
        <n-input
          v-model:value="dirFmt"
          size="small"
          @blur="store.config.dirFmt = dirFmt"
          @keydown.enter="store.config.dirFmt = dirFmt" />
      </template>
    </n-tooltip>

    <span class="font-bold mt-2">其他</span>
    <n-checkbox class="w-fit" v-model:checked="store.config.shouldDownloadCover">下载封面</n-checkbox>
  </div>
</template>
