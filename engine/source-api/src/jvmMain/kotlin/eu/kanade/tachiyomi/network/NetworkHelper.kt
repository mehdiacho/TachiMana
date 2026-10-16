package eu.kanade.tachiyomi.network

/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * Vendored into TachiMana and trimmed: the FlareSolverr-backed CloudflareInterceptor and the
 * GetSource header-reset hook are Suwayomi-server internals and are omitted here. The remaining
 * client wiring is the extensions-lib ABI surface (network.client, defaultUserAgentProvider,
 * cookieStore) that Tachiyomi extensions depend on.
 */

import android.content.Context
import eu.kanade.tachiyomi.network.interceptor.CloudflareInterceptor
import eu.kanade.tachiyomi.network.interceptor.UncaughtExceptionInterceptor
import eu.kanade.tachiyomi.network.interceptor.UserAgentInterceptor
import io.github.oshai.kotlinlogging.KotlinLogging
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.Cache
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import java.net.CookieHandler
import java.net.CookieManager
import java.net.CookiePolicy
import java.nio.file.Files
import java.util.concurrent.TimeUnit

class NetworkHelper(
    context: Context,
) {
    val cookieStore = PersistentCookieStore(context)

    init {
        CookieHandler.setDefault(
            CookieManager(cookieStore, CookiePolicy.ACCEPT_ALL),
        )
    }

    private val userAgent =
        MutableStateFlow(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) " +
                "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
    val userAgentFlow = userAgent.asStateFlow()

    fun defaultUserAgentProvider(): String = userAgent.value

    /** Adopt a user-agent harvested elsewhere (e.g. the JCEF Cloudflare solve), so retried
     *  requests match the fingerprint the clearance cookie was issued for. */
    fun setUserAgent(ua: String) {
        if (ua.isNotBlank()) userAgent.value = ua
    }

    private val baseClientBuilder: OkHttpClient.Builder
        get() {
            val builder =
                OkHttpClient
                    .Builder()
                    .cookieJar(PersistentCookieJar(cookieStore))
                    .connectTimeout(30, TimeUnit.SECONDS)
                    .readTimeout(30, TimeUnit.SECONDS)
                    .callTimeout(2, TimeUnit.MINUTES)
                    .cache(
                        Cache(
                            directory = Files.createTempDirectory("tachimana_network_cache").toFile(),
                            maxSize = 5L * 1024 * 1024, // 5 MiB
                        ),
                    ).addInterceptor(UncaughtExceptionInterceptor())
                    .addInterceptor(UserAgentInterceptor(::defaultUserAgentProvider))
                    // Required by name: extensions verify the default client contains a
                    // CloudflareInterceptor (see its KDoc) before they will run.
                    .addInterceptor(CloudflareInterceptor())

            val httpLoggingInterceptor =
                HttpLoggingInterceptor(
                    object : HttpLoggingInterceptor.Logger {
                        val logger = KotlinLogging.logger { }

                        override fun log(message: String) {
                            logger.debug { message }
                        }
                    },
                ).apply {
                    level = HttpLoggingInterceptor.Level.BASIC
                }
            builder.addNetworkInterceptor(httpLoggingInterceptor)

            return builder
        }

    val client by lazy { baseClientBuilder.build() }

    @Deprecated("The regular client handles requests directly")
    @Suppress("UNUSED")
    val cloudflareClient by lazy { client }
}
