package me.faulk.codeplugme.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

// A radio-equipment palette: desaturated teal, so the destructive red used on
// "Write to Radio" stands out against everything else in the app rather than
// competing with a colourful theme.
private val Teal = Color(0xFF1F4E5F)
private val TealLight = Color(0xFF7FC8DC)

private val LightColors = lightColorScheme(
    primary = Teal,
    secondary = Color(0xFF4A6572),
)

private val DarkColors = darkColorScheme(
    primary = TealLight,
    secondary = Color(0xFFA7C0CD),
)

/** The app's Material 3 theme, following the system's light/dark setting. */
@Composable
fun CodeplugTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val colors = when {
        // Material You where the platform offers it; the fixed palette otherwise.
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)

        darkTheme -> DarkColors
        else -> LightColors
    }
    MaterialTheme(colorScheme = colors, content = content)
}
