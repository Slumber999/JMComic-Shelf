// 由 jm-tools/android-patch-android.ps1 注入：把导出好的 zip 流式复制进手机公共下载目录。
// src-tauri/gen/android 是生成目录，重新 init 后必须再跑一次本脚本。
package com.slumber999.jmcomic_shelf

import android.content.ContentResolver
import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import java.io.File
import java.io.FileInputStream
import java.io.IOException
import java.io.OutputStream

// API 29+ 走 MediaStore.Downloads（不需要任何权限）；API < 29 或写入失败返回 null，
// 由 Rust 回退到 app 私有目录里的那份 zip。Rust 按类名 JNI 调用，R8 规则见 proguard-rules.pro。
object DownloadSink {
    private const val MIME_ZIP = "application/zip"

    // 下载目录下建一个应用自己的子目录，别把 zip 直接撒在用户的 Download 里
    private const val SUBDIR = "禁漫书架"

    // 256KB 分块：不能把整个 zip 读进内存
    private const val BUFFER_SIZE = 256 * 1024

    @JvmStatic
    fun publishToDownloads(context: Context, sourcePath: String, displayName: String): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return null
        }
        val source = File(sourcePath)
        if (!source.isFile) {
            return null
        }
        val resolver = context.contentResolver
        val pending = ContentValues().apply {
            put(MediaStore.Downloads.DISPLAY_NAME, displayName)
            put(MediaStore.Downloads.MIME_TYPE, MIME_ZIP)
            put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/" + SUBDIR)
            // 先标 pending 再写，写完才发布，下载目录里不会出现半截 zip
            put(MediaStore.Downloads.IS_PENDING, 1)
        }
        val uri = try {
            resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, pending)
        } catch (error: Throwable) {
            null
        } ?: return null
        return try {
            FileInputStream(source).use { input ->
                val output = resolver.openOutputStream(uri)
                    ?: throw IOException("openOutputStream 返回 null")
                output.use { copyStream(input, it) }
            }
            val done = ContentValues().apply { put(MediaStore.Downloads.IS_PENDING, 0) }
            resolver.update(uri, done, null, null)
            "Download/" + SUBDIR + "/" + (queryDisplayName(resolver, uri) ?: displayName)
        } catch (error: Throwable) {
            // 半成品要删掉，否则下载目录里留一个 0 字节文件
            runCatching { resolver.delete(uri, null, null) }
            null
        }
    }

    private fun copyStream(input: FileInputStream, output: OutputStream) {
        val buffer = ByteArray(BUFFER_SIZE)
        while (true) {
            val read = input.read(buffer)
            if (read <= 0) break
            output.write(buffer, 0, read)
        }
        output.flush()
    }

    // 同名文件 MediaStore 会自动改名（xxx (1).zip），把真实名字返回给用户看
    private fun queryDisplayName(resolver: ContentResolver, uri: Uri): String? =
        runCatching {
            resolver.query(uri, arrayOf(MediaStore.Downloads.DISPLAY_NAME), null, null, null)
                ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
        }.getOrNull()
}
