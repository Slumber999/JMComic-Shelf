// 由 jm-tools/android-patch-android.ps1 注入：返回键优先交给 WebView 历史、系统栏与应用同色（去蓝边）。
// src-tauri/gen/android 是生成目录，重新 init 后必须再跑一次本脚本。
package com.slumber999.jmcomic_shelf

import android.os.Build
import android.os.Bundle
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsetsController
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        applySystemBars()
        // onBackPressedDispatcher 在主线程分发返回：WebView 还能回退就回退，否则交回系统默认实现
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    val webView = findWebView(window.decorView)
                    if (webView != null && webView.canGoBack()) {
                        webView.goBack()
                    } else {
                        // 先停用回调再分发，避免递归，最终走系统默认处理（退出应用）
                        isEnabled = false
                        onBackPressedDispatcher.onBackPressed()
                        isEnabled = true
                    }
                }
            },
        )
    }

    // 状态栏/导航栏跟应用背景一致（白底深色图标），避免状态栏保留主题默认的紫色主色
    private fun applySystemBars() {
        window.statusBarColor = BAR_COLOR
        window.navigationBarColor = BAR_COLOR
        // decorView 还没 attach 时拿 insetsController 会空指针：等一帧再设图标明暗
        window.decorView.post {
            runCatching {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    window.insetsController?.setSystemBarsAppearance(
                        WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                            WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
                        WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                            WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS,
                    )
                } else {
                    @Suppress("DEPRECATION")
                    window.decorView.systemUiVisibility =
                        View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
                }
            }
        }
    }

    // RustWebView 继承自 android.webkit.WebView，在 window.decorView 下递归查找
    private fun findWebView(view: View?): WebView? {
        if (view is WebView) return view
        if (view is ViewGroup) {
            for (index in 0 until view.childCount) {
                findWebView(view.getChildAt(index))?.let { return it }
            }
        }
        return null
    }

    companion object {
        // 与应用顶栏/页面背景相同的白色（浅色主题）
        private const val BAR_COLOR = 0xFFFFFFFF.toInt()
    }
}