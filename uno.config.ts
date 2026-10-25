import {
    defineConfig,
    presetAttributify,
    presetIcons,
    presetTypography,
    presetUno,
    presetWebFonts,
    transformerDirectives,
    transformerVariantGroup
} from 'unocss'

export default defineConfig({
    shortcuts: [
        // ...
    ],
    theme: {
        colors: {
            // 禁漫橙：站内主色，桌面与移动共用的唯一色值来源
            // - orange 是主色，deep 是按下 / 当前章节那种更深一档的橙
            jm: {
                orange: '#FF7A00',
                deep: '#D96200',
            },
        }
    },
    presets: [
        presetUno(),
        presetAttributify(),
        presetIcons(),
        presetTypography(),
        presetWebFonts({
            fonts: {
                // ...
            },
        }),
    ],
    transformers: [
        transformerDirectives(),
        transformerVariantGroup(),
    ],
})