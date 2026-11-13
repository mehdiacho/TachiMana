package android.webkit;

/**
 * Why a WebView's render process went away, as the Android API defines it. The android.jar stub
 * leaves it out, and {@link WebViewClient#onRenderProcessGone} needs it.
 */
public abstract class RenderProcessGoneDetail {
    /** True if the render process crashed, false if the system killed it. */
    public abstract boolean didCrash();

    /** The renderer's priority when it exited. */
    public abstract int rendererPriorityAtExit();
}
