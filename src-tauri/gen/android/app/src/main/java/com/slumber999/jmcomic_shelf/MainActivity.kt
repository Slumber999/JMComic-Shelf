// 由 jm-tools/android-patch-android.ps1 注入 v7：返回键优先交给 WebView 历史、系统栏与应用同色（去蓝边）、
// 未捕获异常追加写入 filesDir/crash-java.log、启动时把上次崩溃/被系统杀死的原因 WARN 到 logcat（tag JMCRASH）、
// 低内存时收窄阅读器挂载窗口、窗口方向锁定竖屏（Manifest 已声明，这里双保险）、
// 阅读器打开时音量键翻页（readerActive 由 JS 经 JMGallery 开关）+ JMGallery 桥把 base64 图片存进系统相册。
// src-tauri/gen/android 是生成目录，重新 init 后必须再跑一次本脚本。
package com.slumber999.jmcomic_shelf

import android.Manifest
import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.ComponentCallbacks2
import android.content.ContentValues
import android.content.Context
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.media.MediaScannerConnection
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import android.util.Log
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsetsController
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.annotation.RequiresApi
import androidx.core.content.ContextCompat
import java.io.File
import java.io.IOException
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

class MainActivity : TauriActivity() {
    // 阅读器是否打开：只有阅读页里音量键才翻页，其余时候音量键照常调系统音量（JS 通过 JMGallery.setReaderActive 设置）
    @Volatile
    private var readerActive = false

    // API < 29 且还没拿到 WRITE_EXTERNAL_STORAGE 时，把这次要写的图片暂存下来，授权回调里接着写
    private var pendingSave: PendingSave? = null

    private class PendingSave(val bytes: ByteArray, val mime: String, val name: String)

    override fun onCreate(savedInstanceState: Bundle?) {
        // 取证顺序：先看上次进程怎么没的（native 崩溃/LMK/ANR 不走 Java handler，只有这里查得到），
        // 再把上次的 Java 崩溃吐到 logcat，最后装 handler；super.onCreate 放最后，WebView 初始化崩了也能留痕
        reportPreviousExit()
        reportCrashLogGrowth()
        installCrashHandler()
        super.onCreate(savedInstanceState)
        // 双保险：Manifest 已声明 android:screenOrientation="portrait"，这里再锁一次，
        // 防止某些 ROM 或父类在启动后把窗口方向改回去（只影响本应用，不碰系统旋转开关）
        requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        applySystemBars()
        // JS 桥：Tauri 的 WebView 在 super.onCreate 之后才挂，建好前要重试
        attachGalleryBridge()
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

    // WebView 创建完成的回调：此时页面还没加载，桥从这里挂最稳（晚挂的话已加载的 JS 里看不到）
    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        runCatching { webView.addJavascriptInterface(GalleryBridge(this@MainActivity), "JMGallery") }
            .onSuccess {
                galleryBridgeAttached = true
                Log.i(GALLERY_TAG, "JMGallery 桥已挂载（onWebViewCreate）")
            }
            .onFailure { Log.w(GALLERY_TAG, "挂载 JMGallery 桥失败：" + Log.getStackTraceString(it)) }
    }

    // 兜底：onWebViewCreate 没触发时（个别 Tauri 版本）再按帧重试
    // WebView 建好有延迟：没找到就隔 200ms 重试（最多 50 次），挂上后打一条日志
    private var galleryBridgeAttempts = 0
    private var galleryBridgeAttached = false

    private fun attachGalleryBridge() {
        if (galleryBridgeAttached) {
            return
        }
        window.decorView.post {
            if (galleryBridgeAttached) {
                return@post
            }
            if (galleryBridgeAttempts >= 50) {
                Log.w(GALLERY_TAG, "重试 50 次仍没找到 WebView，JMGallery 桥放弃挂载")
                return@post
            }
            galleryBridgeAttempts += 1
            val webView = runCatching { findWebView(window.decorView) }.getOrNull()
            if (webView == null) {
                window.decorView.postDelayed({ attachGalleryBridge() }, 200)
                return@post
            }
            runCatching { webView.addJavascriptInterface(GalleryBridge(this@MainActivity), "JMGallery") }
                .onSuccess {
                    galleryBridgeAttached = true
                    Log.i(GALLERY_TAG, "JMGallery 桥已挂载（第 " + galleryBridgeAttempts + " 次尝试）")
                }
                .onFailure { Log.w(GALLERY_TAG, "挂载 JMGallery 桥失败：" + Log.getStackTraceString(it)) }
        }
    }

    override fun onPostResume() {
        super.onPostResume()
        // Activity 回到前台再兜一次：某些机型要等 resume 之后 WebView 才真正 attach
        attachGalleryBridge()
    }

    // 音量键翻页：阅读器打开时把音量键转成 JS 的上一页(-1)/下一页(1) 并吃掉按键；
    // 其余情况（阅读器没开、其它按键、ACTION_UP）原样交给系统，音量键照常工作。
    // 按住音量键会连续产生 ACTION_DOWN，节流交给 JS 侧做。
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && readerActive) {
            val direction = when (event.keyCode) {
                KeyEvent.KEYCODE_VOLUME_UP -> -1
                KeyEvent.KEYCODE_VOLUME_DOWN -> 1
                else -> 0
            }
            if (direction != 0) {
                // 找不到 WebView 也照样吃掉按键：阅读器开着时按音量键不该改变系统音量
                runCatching {
                    findWebView(window.decorView)?.evaluateJavascript(
                        "window.__jmVolumeKey&&window.__jmVolumeKey($direction)",
                        null,
                    )
                }.onFailure { Log.w(GALLERY_TAG, "音量键转发给 JS 失败：" + Log.getStackTraceString(it)) }
                return true
            }
        }
        return super.dispatchKeyEvent(event)
    }

    // API < 29 的相册写入要运行时权限：授权回调里把 saveImage 暂存的那张图补写完（当时已同步返回 "permission"）
    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode != LEGACY_WRITE_REQUEST) {
            return
        }
        val pending = pendingSave ?: return
        pendingSave = null
        val granted = grantResults.isNotEmpty() && grantResults[0] == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            Log.w(GALLERY_TAG, "缺少写入相册的权限，放弃本次保存")
            return
        }
        val result = saveLegacyImage(pending)
        if (result != SAVE_OK) {
            Log.w(GALLERY_TAG, "授权后写入相册失败：" + result)
        }
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

    // ---------- JMGallery 桥的实现：JS 传来的 base64 图片落进系统相册 ----------
    // 同步返回 "ok" 或错误原因（permission / insert / write / decode / empty），全程不抛异常

    private fun saveImageToGallery(base64: String, mime: String, name: String): String {
        if (base64.isBlank()) {
            return "empty"
        }
        return try {
            val bytes = Base64.decode(stripDataUrl(base64), Base64.DEFAULT)
            if (bytes.isEmpty()) {
                return "decode"
            }
            val safeMime = mime.ifBlank { "image/jpeg" }
            val safeName = withExtension(sanitizeFileName(name), safeMime)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                saveViaMediaStore(bytes, safeMime, safeName)
            } else {
                saveToPublicPictures(bytes, safeMime, safeName)
            }
        } catch (error: Throwable) {
            Log.w(GALLERY_TAG, "保存图片失败：" + Log.getStackTraceString(error))
            "write"
        }
    }

    // JS 可能传 data URL，也可能传裸 base64，两种都吃
    private fun stripDataUrl(base64: String): String {
        val trimmed = base64.trim()
        val comma = trimmed.indexOf(',')
        return if (trimmed.startsWith("data:") && comma > 0) {
            trimmed.substring(comma + 1)
        } else {
            trimmed
        }
    }

    // API 29+：MediaStore 直写 Pictures/jmcomic-shelf，先 pending 写完再发布，不需要任何权限
    @RequiresApi(Build.VERSION_CODES.Q)
    private fun saveViaMediaStore(bytes: ByteArray, mime: String, name: String): String {
        val values = ContentValues().apply {
            put(MediaStore.Images.Media.DISPLAY_NAME, name)
            put(MediaStore.Images.Media.MIME_TYPE, mime)
            put(MediaStore.Images.Media.RELATIVE_PATH, Environment.DIRECTORY_PICTURES + "/" + ALBUM)
            // 先标 pending 再写，写完才发布，相册里不会出现半截图片
            put(MediaStore.Images.Media.IS_PENDING, 1)
        }
        val uri = runCatching {
            contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
        }.getOrNull() ?: return "insert"
        val written = runCatching {
            val output = contentResolver.openOutputStream(uri) ?: throw IOException("openOutputStream 返回 null")
            output.use { it.write(bytes) }
        }
        if (written.isFailure) {
            Log.w(GALLERY_TAG, "写入相册失败：" + Log.getStackTraceString(written.exceptionOrNull()))
            // 半成品要删掉，否则相册里留一个 0 字节的图
            runCatching { contentResolver.delete(uri, null, null) }
            return "write"
        }
        val published = runCatching {
            val done = ContentValues().apply { put(MediaStore.Images.Media.IS_PENDING, 0) }
            contentResolver.update(uri, done, null, null)
        }
        if (published.isFailure) {
            runCatching { contentResolver.delete(uri, null, null) }
            return "write"
        }
        return SAVE_OK
    }

    // API < 29：没有 WRITE_EXTERNAL_STORAGE 就先申请，数据暂存到 pendingSave，同步返回 "permission"
    private fun saveToPublicPictures(bytes: ByteArray, mime: String, name: String): String {
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.WRITE_EXTERNAL_STORAGE,
        ) == PackageManager.PERMISSION_GRANTED
        if (!granted) {
            pendingSave = PendingSave(bytes, mime, name)
            // JS 桥方法跑在 WebView 的 JavaBridge 线程，申请权限必须回主线程
            runOnUiThread {
                runCatching {
                    requestPermissions(arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), LEGACY_WRITE_REQUEST)
                }.onFailure {
                    pendingSave = null
                    Log.w(GALLERY_TAG, "申请写入相册权限失败：" + Log.getStackTraceString(it))
                }
            }
            return "permission"
        }
        return saveLegacyImage(PendingSave(bytes, mime, name))
    }

    // 直接写进 getExternalStoragePublicDirectory(PICTURES)/jmcomic-shelf，再用 MediaScanner 通知相册
    @Suppress("DEPRECATION")
    private fun saveLegacyImage(pending: PendingSave): String {
        return try {
            val pictures = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_PICTURES)
            val dir = File(pictures, ALBUM)
            if (!dir.isDirectory) {
                val created = runCatching { dir.mkdirs() }.getOrDefault(false)
                if (!created && !dir.isDirectory) {
                    return "write"
                }
            }
            val target = uniqueFile(dir, pending.name)
            try {
                target.outputStream().use { it.write(pending.bytes) }
            } catch (error: Throwable) {
                runCatching { target.delete() }
                Log.w(GALLERY_TAG, "写相册文件失败：" + Log.getStackTraceString(error))
                return "write"
            }
            // 让系统相册立刻看到；扫描失败不影响已经落盘的文件
            runCatching {
                MediaScannerConnection.scanFile(this, arrayOf(target.absolutePath), arrayOf(pending.mime), null)
            }.onFailure { Log.w(GALLERY_TAG, "通知相册扫描失败：" + Log.getStackTraceString(it)) }
            SAVE_OK
        } catch (error: Throwable) {
            Log.w(GALLERY_TAG, "保存图片失败：" + Log.getStackTraceString(error))
            "write"
        }
    }

    // 重名就加时间戳后缀（同一秒仍重名再补序号），绝不覆盖用户已有的图
    private fun uniqueFile(dir: File, name: String): File {
        val candidate = File(dir, name)
        if (!candidate.exists()) {
            return candidate
        }
        val hasExtension = name.contains('.')
        val stem = if (hasExtension) name.substringBeforeLast('.') else name
        val suffix = if (hasExtension) "." + name.substringAfterLast('.') else ""
        val stamp = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())
        var file = File(dir, "$stem-$stamp$suffix")
        var index = 1
        while (file.exists() && index <= 100) {
            file = File(dir, "$stem-$stamp-$index$suffix")
            index += 1
        }
        return file
    }

    // 文件名只留个干净的名字：去掉路径分隔符、控制字符和文件系统不认的符号
    private fun sanitizeFileName(name: String): String {
        val base = name.substringAfterLast('/').substringAfterLast('\\').trim()
        val cleaned = base.map { ch ->
            if (ch.code < 0x20 || ch in "\\/:*?\"<>|") '_' else ch
        }.joinToString("")
        return cleaned.trim().ifBlank { defaultFileName() }
    }

    private fun withExtension(name: String, mime: String): String {
        val base = name.ifBlank { defaultFileName() }
        if (base.contains('.')) {
            return base
        }
        return base + extensionFor(mime)
    }

    private fun extensionFor(mime: String): String = when (mime.lowercase(Locale.US)) {
        "image/png" -> ".png"
        "image/webp" -> ".webp"
        "image/gif" -> ".gif"
        "image/avif" -> ".avif"
        "image/heic", "image/heif" -> ".heic"
        else -> ".jpg"
    }

    private fun defaultFileName(): String =
        "jmcomic-" + SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())

    // JS 桥（window.JMGallery）：方法必须带 @JavascriptInterface 才会暴露给网页
    class GalleryBridge(private val host: MainActivity) {
        @JavascriptInterface
        fun setReaderActive(active: Boolean) {
            host.readerActive = active
        }

        @JavascriptInterface
        fun isAvailable(): Boolean = true

        // 同步返回 "ok" 或错误原因（permission / insert / write），JS 侧据此提示
        @JavascriptInterface
        fun saveImage(base64: String, mime: String, name: String): String =
            host.saveImageToGallery(base64, mime, name)
    }

    companion object {
        // 与应用顶栏/页面背景相同的白色（浅色主题）
        private const val BAR_COLOR = 0xFFFFFFFF.toInt()
        // 闪退兜底文件（filesDir 下，adb 或应用内日志入口都能读到）
        private const val CRASH_LOG = "crash-java.log"
        private const val PREFS = "crash-diagnostics"
        private const val KEY_SEEN_SIZE = "java-crash-seen-bytes"
        private const val CRASH_TAG = "JMCRASH"
        // 相册子目录（Pictures/jmcomic-shelf）
        private const val ALBUM = "jmcomic-shelf"
        // JMGallery 桥的日志 tag
        private const val GALLERY_TAG = "JMGALLERY"
        // saveImage 成功返回值
        private const val SAVE_OK = "ok"
        // requestPermissions 请求码，发起和回调两边对上就行
        private const val LEGACY_WRITE_REQUEST = 4801
    }
}
