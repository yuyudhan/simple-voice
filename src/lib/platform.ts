// FilePath: src/lib/platform.ts
// The desktop the app runs on, for the few places where macOS and Linux differ in what the user
// sees: WebKitGTK (Linux) puts "Linux" in its user agent, WKWebView (macOS) "Macintosh".
export const isLinux = typeof navigator !== "undefined" && /\bLinux\b/.test(navigator.userAgent);
