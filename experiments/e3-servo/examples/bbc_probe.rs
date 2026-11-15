//! Diagnoses live-site compatibility through Servo's public API, without Bulwark.
//! See docs/phase-2-bbc-investigation.md for commands and evidence limits.
use servo::{
    ConsoleLogLevel, EventLoopWaker, RenderingContext, ServoBuilder, SoftwareRenderingContext,
    WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};
#[derive(Clone)]
struct Waker;
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {}
}
struct Delegate;
impl WebViewDelegate for Delegate {
    fn show_console_message(&self, _: WebView, level: ConsoleLogLevel, message: String) {
        println!("console {level:?}: {message}");
    }
    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        println!("title: {title:?}");
    }
    fn notify_crashed(&self, _: WebView, reason: String, backtrace: Option<String>) {
        println!("crash: {reason} {backtrace:?}");
    }
}
const INSPECT: &str = r#"
JSON.stringify({
    url: location.href,
    title: document.title,
    body: document.body.innerText.slice(0, 1500),
    ua: navigator.userAgent,
    secureContext: isSecureContext,
    crypto: typeof crypto,
    getRandomValues: typeof globalThis.crypto?.getRandomValues,
    intersectionObserver: typeof IntersectionObserver,
    h1: Array.from(document.querySelectorAll('h1')).map(e => e.textContent),
    scrollHeight: document.documentElement.scrollHeight
})
"#;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    for arg in &args {
        anyhow::ensure!(
            !arg.starts_with("--")
                || matches!(arg.as_str(), "--diagnostics" | "--intersection-observer"),
            "Usage: bbc_probe [URL] [--diagnostics] [--intersection-observer]"
        );
    }
    anyhow::ensure!(
        args.iter().filter(|arg| !arg.starts_with("--")).count() <= 1,
        "Expected at most one URL"
    );
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let context = Rc::new(
        SoftwareRenderingContext::new((1000, 800).into())
            .map_err(|error| anyhow::anyhow!("Software context: {error:?}"))?,
    );
    context
        .make_current()
        .map_err(|error| anyhow::anyhow!("Current context: {error:?}"))?;
    let mut preferences = servo::Preferences::default();
    if args.iter().any(|arg| arg == "--intersection-observer") {
        preferences.dom_intersection_observer_enabled = true;
    }
    let servo = ServoBuilder::default()
        .preferences(preferences)
        .event_loop_waker(Box::new(Waker))
        .build();
    let user_content = Rc::new(servo::UserContentManager::new(&servo));
    if args.iter().any(|arg| arg == "--diagnostics") {
        user_content.add_script(Rc::new(servo::UserScript::new(r#"
            (() => {
            if (console.error.__bbcProbe) return;
            const originalError = console.error;
            console.error = function(...args) {
                originalError.apply(console, args.map(value => value instanceof Error ? String(value) + '\n' + value.stack : value));
            };
            console.error.__bbcProbe = true;
            addEventListener('error', event => console.error('probe error', event.message, event.error));
            addEventListener('unhandledrejection', event => console.error('probe rejection', event.reason));
            })();
        "#.into(), None)));
    }
    let url = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "https://www.bbc.com/news".into());
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url.parse()?)
        .user_content_manager(user_content)
        .delegate(Rc::new(Delegate))
        .build();
    view.show();
    let started = Instant::now();
    let mut inspected = false;
    while started.elapsed() < Duration::from_secs(30) {
        servo.spin_event_loop();
        view.paint();
        if !inspected && started.elapsed() > Duration::from_secs(20) {
            inspected = true;
            view.evaluate_javascript(INSPECT, |result| match result {
                Ok(servo::JSValue::String(json)) => println!("inspection: {json}"),
                other => eprintln!("inspection failed: {other:?}"),
            });
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
