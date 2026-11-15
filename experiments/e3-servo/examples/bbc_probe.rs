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
    let mut args = std::env::args().skip(1);
    let mut url = None;
    let mut navigate = None;
    let mut seconds = None;
    let mut diagnostics = false;
    let mut user_script = None;
    let mut screenshot = None;
    let mut intersection_observer = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--diagnostics" => diagnostics = true,
            "--user-script" => {
                user_script = Some(std::path::PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing script path"))?,
                ));
            }
            "--screenshot" => {
                screenshot = Some(std::path::PathBuf::from(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing screenshot path"))?,
                ));
            }
            "--intersection-observer" => intersection_observer = true,
            "--seconds" => {
                seconds = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing seconds"))?
                        .parse::<u64>()?,
                )
            }
            "--navigate" => {
                navigate = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("Missing navigation URL"))?
                        .parse::<url::Url>()?,
                )
            }
            _ if !arg.starts_with("--") && url.is_none() => url = Some(arg.parse::<url::Url>()?),
            _ => anyhow::bail!(
                "Usage: bbc_probe [URL] [--diagnostics] [--intersection-observer] [--seconds N] [--navigate URL] [--user-script PATH] [--screenshot PATH]"
            ),
        }
    }
    let seconds = seconds.unwrap_or(if navigate.is_some() { 90 } else { 30 });
    anyhow::ensure!(
        (21..=300).contains(&seconds),
        "Duration must be 21 to 300 seconds"
    );
    anyhow::ensure!(
        navigate.is_none() || seconds >= 71,
        "Navigation checks need at least 71 seconds to observe the reload"
    );
    let url = url.unwrap_or("https://www.bbc.com/news".parse()?);
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let context = Rc::new(
        SoftwareRenderingContext::new((1000, 800).into())
            .map_err(|error| anyhow::anyhow!("Software context: {error:?}"))?,
    );
    context
        .make_current()
        .map_err(|error| anyhow::anyhow!("Current context: {error:?}"))?;
    let mut preferences = servo::Preferences::default();
    if intersection_observer {
        preferences.dom_intersection_observer_enabled = true;
    }
    let servo = ServoBuilder::default()
        .preferences(preferences)
        .event_loop_waker(Box::new(Waker))
        .build();
    let user_content = Rc::new(servo::UserContentManager::new(&servo));
    if diagnostics {
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
    if let Some(path) = user_script {
        user_content.add_script(Rc::new(servo::UserScript::new(
            std::fs::read_to_string(&path)?,
            Some(path),
        )));
    }
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url)
        .user_content_manager(user_content)
        .delegate(Rc::new(Delegate))
        .build();
    view.show();
    let started = Instant::now();
    let mut inspect_at = Duration::from_secs(20);
    let mut navigated = false;
    let mut reloaded = false;
    while started.elapsed() < Duration::from_secs(seconds) {
        servo.spin_event_loop();
        view.paint();
        if !navigated
            && started.elapsed() > Duration::from_secs(30)
            && let Some(url) = &navigate
        {
            println!("navigation at {:?}: {url}", started.elapsed());
            view.load(url.clone());
            navigated = true;
        }
        if navigated && !reloaded && started.elapsed() > Duration::from_secs(60) {
            println!("reload at {:?}", started.elapsed());
            view.reload();
            reloaded = true;
        }
        if started.elapsed() > inspect_at {
            println!("inspection time: {:?}", started.elapsed());
            inspect_at += Duration::from_secs(10);
            view.evaluate_javascript(INSPECT, |result| match result {
                Ok(servo::JSValue::String(json)) => println!("inspection: {json}"),
                other => eprintln!("inspection failed: {other:?}"),
            });
        }
        if started.elapsed() > Duration::from_secs(seconds - 10)
            && let Some(path) = screenshot.take()
        {
            view.take_screenshot(None, move |result| match result {
                Ok(image) => match image.save(&path) {
                    Ok(()) => println!("screenshot: {}", path.display()),
                    Err(error) => eprintln!("screenshot save failed: {error}"),
                },
                Err(error) => eprintln!("screenshot failed: {error:?}"),
            });
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
