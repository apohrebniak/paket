package paket.paket

import android.content.Context
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                MainScreen()
            }
        }
    }
}

@Composable
fun MainScreen() {
    val context = LocalContext.current
    val prefs = context.getSharedPreferences("LinkSharePrefs", Context.MODE_PRIVATE)

    var url by remember { mutableStateOf(prefs.getString("url", "") ?: "") }
    var headers by remember { mutableStateOf(prefs.getString("headers", "") ?: "") }

    Scaffold { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            Text(
                text = "URL",
                style = MaterialTheme.typography.titleMedium
            )

            OutlinedTextField(
                value = url,
                onValueChange = { url = it },
                modifier = Modifier.fillMaxWidth(),
                placeholder = { Text("https://example.com/api/share") },
                singleLine = true
            )

            Text(
                text = "HTTP Headers (JSON format)",
                style = MaterialTheme.typography.titleMedium
            )

            OutlinedTextField(
                value = headers,
                onValueChange = { headers = it },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(120.dp),
                placeholder = {
                    Text("{\"Authorization\": \"Bearer token\", \"Content-Type\": \"application/json\"}")
                },
                minLines = 3
            )

            Button(
                onClick = {
                    prefs.edit().apply {
                        putString("url", url)
                        putString("headers", headers)
                        apply()
                    }
                    Toast.makeText(context, "Settings saved!", Toast.LENGTH_SHORT).show()
                },
                modifier = Modifier.fillMaxWidth()
            ) {
                Text("Save Settings")
            }
        }
    }
}
