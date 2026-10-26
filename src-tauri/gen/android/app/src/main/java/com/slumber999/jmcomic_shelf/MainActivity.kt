// 由 jm-tools/android-patch-android.ps1 注入 v3：返回键优先交给 WebView 历史、系统栏与应用同色（去蓝边）、
// 未捕获异常追加写入 filesDir/crash-java.log、启动时把上次崩溃/被系统杀死的原因 WARN 到 logcat（tag JMCRASH）、
// 低内存时收窄阅读器挂载窗口。
// src-tauri/gen/android 是生成目录，重新 init 后必须再跑一次本脚本。
package com.slumber999.jmcomic_shelf

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.ComponentCallbacks2
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsetsController
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // 取证顺序：先看上次进程怎么没的（native 崩溃/LMK/ANR 不走 Java handler，只有这里查得到），
        // 再把上次的 Java 崩溃吐到 logcat，最后装 handler；super.onCreate 放最后，WebView 初始化崩了也能留痕
        reportPreviousExit()
        reportCrashLogGrowth()
        installCrashHandler()
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

    // 系统内存吃紧时通知前端收窄阅读器挂载窗口：图片解码是大头，按系统水位取舍而不是固定页数
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level < ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) {
            return
        }
        runCatching {
            findWebView(window.decorView)?.evaluateJavascript(
                "window.__jmOnLowMemory&&window.__jmOnLowMemory()",
                null,
            )
        }
    }

    // 上次进程的退出原因（ActivityManager 历史记录）：只报异常类，正常退出不刷日志。
    // 原生崩溃/被 LMK 杀/ANR 时把 tombstone 或 ANR trace 前 4000 字一起打出来
    private fun reportPreviousExit() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return
        }
        val exits = runCatching {
            getSystemService(ActivityManager::class.java)?.getHistoricalProcessExitReasons(packageName, 0, 5)
        }.getOrNull() ?: return
        for (info in exits) {
            if (!isAbnormalExit(info.reason)) {
                continue
            }
            val trace = runCatching {
                info.traceInputStream?.use { input -> input.readBytes().toString(Charsets.UTF_8).take(4000) }
            }.getOrNull().orEmpty()
            Log.w(
                CRASH_TAG,
                "上次进程异常退出：reason=${exitReasonName(info.reason)}" +
                    " importance=${info.importance} time=${info.timestamp} desc=${info.description.orEmpty()}\n$trace",
            )
            return
        }
    }

    private fun isAbnormalExit(reason: Int): Boolean = when (reason) {
        ApplicationExitInfo.REASON_SIGNALED,
        ApplicationExitInfo.REASON_LOW_MEMORY,
        ApplicationExitInfo.REASON_CRASH,
        ApplicationExitInfo.REASON_CRASH_NATIVE,
        ApplicationExitInfo.REASON_ANR,
        ApplicationExitInfo.REASON_INITIALIZATION_FAILURE,
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE,
        ApplicationExitInfo.REASON_OTHER,
        ApplicationExitInfo.REASON_DEPENDENCY_DIED,
        -> true
        else -> false
    }

    private fun exitReasonName(reason: Int): String = when (reason) {
        ApplicationExitInfo.REASON_EXIT_SELF -> "EXIT_SELF"
        ApplicationExitInfo.REASON_SIGNALED -> "SIGNALED"
        ApplicationExitInfo.REASON_LOW_MEMORY -> "LOW_MEMORY"
        ApplicationExitInfo.REASON_CRASH -> "CRASH"
        ApplicationExitInfo.REASON_CRASH_NATIVE -> "CRASH_NATIVE"
        ApplicationExitInfo.REASON_ANR -> "ANR"
        ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "INITIALIZATION_FAILURE"
        ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "PERMISSION_CHANGE"
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "EXCESSIVE_RESOURCE_USAGE"
        ApplicationExitInfo.REASON_USER_REQUESTED -> "USER_REQUESTED"
        ApplicationExitInfo.REASON_USER_STOPPED -> "USER_STOPPED"
        ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "DEPENDENCY_DIED"
        ApplicationExitInfo.REASON_OTHER -> "OTHER"
        else -> reason.toString()
    }

    // 未捕获异常统一追加落盘：闪退后重开应用，reportCrashLogGrowth 会把新增片段打回 logcat
    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            runCatching {
                val timestamp = SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSS", Locale.US).format(Date())
                File(filesDir, CRASH_LOG).appendText(
                    "\n=== $timestamp thread=${thread.name} pid=${android.os.Process.myPid()} ===\n" +
                        Log.getStackTraceString(error) + "\n",
                )
            }
            if (previous != null) {
                previous.uncaughtException(thread, error)
            } else {
                android.os.Process.killProcess(android.os.Process.myPid())
            }
        }
    }

    // 上次崩过就有新增片段：整段 WARN 到 logcat（不弹窗、不打扰），并把基线推进到文件末尾
    private fun reportCrashLogGrowth() {
        val file = File(filesDir, CRASH_LOG)
        if (!file.isFile) {
            return
        }
        val prefs = getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val seen = prefs.getLong(KEY_SEEN_SIZE, 0L)
        val size = file.length()
        if (size < seen) {
            // 文件被清理/重装过：基线归零，下次从头读
            prefs.edit().putLong(KEY_SEEN_SIZE, 0L).apply()
            return
        }
        if (size == seen) {
            return
        }
        val fresh = runCatching {
            file.inputStream().use { input ->
                input.skip(seen)
                input.readBytes().toString(Charsets.UTF_8)
            }
        }.getOrDefault("")
        Log.w(CRASH_TAG, "检测到新的 Java 崩溃记录（+${size - seen}B，累计 ${size}B）：\n" + fresh.trim().take(2000))
        prefs.edit().putLong(KEY_SEEN_SIZE, size).apply()
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
        // 闪退兜底文件（filesDir 下，adb 或应用内日志入口都能读到）
        private const val CRASH_LOG = "crash-java.log"
        private const val PREFS = "crash-diagnostics"
        private const val KEY_SEEN_SIZE = "java-crash-seen-bytes"
        private const val CRASH_TAG = "JMCRASH"
    }
}
