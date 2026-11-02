// URLs for the `tm` scheme (plan §2.1). Linux and macOS webviews load `tm://kind/...`; Windows'
// WebView2 maps custom schemes to `http://tm.localhost/kind/...`. Every image URL comes from here.

function isWindows(): boolean {
  return typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);
}

export function tmUrl(path: string, windows: boolean = isWindows()): string {
  const clean = path.replace(/^\/+/, "");
  return windows ? `http://tm.localhost/${clean}` : `tm://${clean}`;
}

export function coverUrl(mangaId: number, version: number, windows?: boolean): string {
  return tmUrl(`cover/${mangaId}?v=${version}`, windows);
}

export function pageUrl(chapterId: number, index: number, windows?: boolean): string {
  return tmUrl(`page/${chapterId}/${index}`, windows);
}
