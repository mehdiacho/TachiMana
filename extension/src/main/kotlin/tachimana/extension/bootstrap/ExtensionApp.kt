package tachimana.extension.bootstrap

import android.app.Application
import android.content.Context

/** Minimal emulated Android [Application] the AndroidCompat runtime attaches a context to. */
open class ExtensionApp : Application() {
    override fun onCreate() {
        super.onCreate()
    }

    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(base)
    }
}
