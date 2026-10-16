package eu.kanade.tachiyomi.util

import kotlinx.coroutines.suspendCancellableCoroutine
import rx.Observable
import rx.Subscription
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * JVM actual for [awaitSingle]. Bridges a legacy RxJava 1 [Observable] to a suspend function.
 * (Yōkai's Android actual delegated to a coroutines-rx1 helper; here we bridge directly.)
 */
actual suspend fun <T> Observable<T>.awaitSingle(): T = suspendCancellableCoroutine { cont ->
    var value: T? = null
    var hasValue = false
    val subscription: Subscription = this.single().subscribe(
        { v ->
            value = v
            hasValue = true
        },
        { e -> cont.resumeWithException(e) },
        {
            if (hasValue) {
                @Suppress("UNCHECKED_CAST")
                cont.resume(value as T)
            } else {
                cont.resumeWithException(NoSuchElementException("Observable emitted no items"))
            }
        },
    )
    cont.invokeOnCancellation { subscription.unsubscribe() }
}
