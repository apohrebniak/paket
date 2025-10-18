package paket.paket

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.sp
import okhttp3.FormBody
import okhttp3.Headers
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okio.IOException
import org.json.JSONObject
import kotlin.concurrent.thread
import kotlin.jvm.Throws

class ShareActivity : ComponentActivity() {
    val client = OkHttpClient()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val sharedText = intent.getStringExtra(Intent.EXTRA_TEXT)

        setContent {
            MaterialTheme {
                if (sharedText != null) {
                    SharingScreen(
                        sharedLink = sharedText, onFinish = { finish() })
                } else {
                    finish()
                }
            }
        }
    }

    @Composable
    fun SharingScreen(sharedLink: String, onFinish: () -> Unit) {
        val context = LocalContext.current
        var isProcessing by remember { mutableStateOf(true) }
        var isSuccess by remember { mutableStateOf(false)}
        val handler = remember { Handler(Looper.getMainLooper()) }

        DisposableEffect(sharedLink) {
            val prefs = context.getSharedPreferences("LinkSharePrefs", Context.MODE_PRIVATE)
            val url = prefs.getString("url", "http://localhost:8080/save")!!.toHttpUrl()
            val headers = jsonToHeaders(prefs.getString("headers", "")!!)

            thread {
                try {
                    processLink(sharedLink, url, headers)

                    handler.post {
                        isSuccess = true
                    }

                } catch (e: Exception) {
                    Log.e(null, null, e)
                }

                handler.post {
                    isProcessing = false
                }

                Thread.sleep(500)
                handler.post {
                    onFinish()
                }
            }

            onDispose { }
        }

        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.5f)),
            contentAlignment = Alignment.Center
        ) {
            if (isProcessing) {
                CircularProgressIndicator(
                    color = Color.White
                )
            } else {
                val text = if (isSuccess) {
                    "\uD83D\uDC4D"
                } else {
                    "\uD83D\uDC4E"
                }
                Text(
                    text = text, fontSize = 48.sp, color = Color.White
                )
            }
        }
    }

    @Throws(IOException::class)
    private fun processLink(articleUrl: String, paketUrl: HttpUrl, headers: Headers) {
        articleUrl.toHttpUrlOrNull()?.let {
            val url = it

            val form = FormBody.Builder()
                .add("url", url.toString())
                .build()
            val request = Request.Builder()
                .url(paketUrl)
                .headers(headers)
                .put(form)
                .build()

            client.newCall(request).execute().use { response ->
            }
        }
    }

    private fun jsonToHeaders(jsonString: String): Headers {
        if (jsonString.isEmpty()) {
            return Headers.EMPTY
        }

        val builder = Headers.Builder()

        try {
            val jsonObject = JSONObject(jsonString)
            val keys = jsonObject.keys()

            while (keys.hasNext()) {
                val key = keys.next()
                val value = jsonObject.getString(key)
                builder.add(key, value)
            }
        } catch (e: Exception) {
            // Return empty headers if JSON parsing fails
            return Headers.Builder().build()
        }

        return builder.build()
    }
}
