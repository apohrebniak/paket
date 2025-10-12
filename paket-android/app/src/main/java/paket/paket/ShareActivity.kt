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
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.sp
import kotlin.concurrent.thread

class ShareActivity : ComponentActivity() {
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
        val handler = remember { Handler(Looper.getMainLooper()) }

        DisposableEffect(sharedLink) {
            val prefs = context.getSharedPreferences("LinkSharePrefs", Context.MODE_PRIVATE)
            val url = prefs.getString("url", "") ?: ""
            val headers = prefs.getString("headers", "") ?: ""

            thread {
                // Call placeholder function
                processLink(sharedLink, url, headers)

                // Show checkmark on main thread
                handler.post {
                    isProcessing = false
                }

                // Wait 500ms then close
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
                Text(
                    text = "✓", fontSize = 48.sp, color = Color.White
                )
            }
        }
    }

    private fun processLink(link: String, url: String, headers: String) {
        // Placeholder function - implement your logic here
        // Simulating some processing time
        Thread.sleep(1000)

        // TODO: Implement actual link processing logic
        // For example: send link to configured URL with headers
    }
}
