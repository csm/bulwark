//! Diagnoses live-site compatibility through Servo's public API, without Bulwark.
//! See docs/phase-2-bbc-investigation.md for commands and evidence limits.
use servo::{
    ConsoleLogLevel, EventLoopWaker, RenderingContext, ServoBuilder, SoftwareRenderingContext,
    WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::Cell,
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
    adoptedStyleSheets: typeof document.adoptedStyleSheets,
    skeletons: document.querySelectorAll('.Skeleton').length,
    radioInputs: document.querySelectorAll('input[type=radio]').length,
    radioInsertion: globalThis.reduction ?? null,
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
    let mut adopted_stylesheets = false;
    let mut measure = false;
    let mut await_reduction = false;
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
            "--adopted-stylesheets" => adopted_stylesheets = true,
            "--measure" => measure = true,
            "--await-reduction" => await_reduction = true,
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
                "Usage: bbc_probe [URL] [--diagnostics] [--intersection-observer] [--adopted-stylesheets] [--measure] [--await-reduction] [--seconds N] [--navigate URL] [--user-script PATH] [--screenshot PATH]"
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
    if adopted_stylesheets {
        preferences.dom_adoptedstylesheet_enabled = true;
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
    let mut heartbeat_at = Duration::from_secs(1);
    let mut longest_turn = Duration::ZERO;
    let reduction_done = Rc::new(Cell::new(false));
    let reduction_pending = Rc::new(Cell::new(false));
    let mut reduction_at = Duration::from_millis(100);
    while started.elapsed() < Duration::from_secs(seconds) && !reduction_done.get() {
        let turn = Instant::now();
        servo.spin_event_loop();
        view.paint();
        longest_turn = longest_turn.max(turn.elapsed());
        if await_reduction
            && started.elapsed() >= reduction_at
            && !reduction_pending.get()
            && !reduction_done.get()
        {
            reduction_at = started.elapsed() + Duration::from_millis(100);
            reduction_pending.set(true);
            let pending = reduction_pending.clone();
            let done = reduction_done.clone();
            view.evaluate_javascript(
                "JSON.stringify(globalThis.reduction ?? null)",
                move |result| {
                    pending.set(false);
                    if let Ok(servo::JSValue::String(json)) = result
                        && json != "null"
                        && !done.replace(true)
                    {
                        println!("reduction: {json}");
                    }
                },
            );
        }
        if measure && started.elapsed() >= heartbeat_at {
            heartbeat_at = started.elapsed() + Duration::from_secs(1);
            let requested = Instant::now();
            view.evaluate_javascript("0", move |result| {
                println!(
                    "heartbeat: ms={} ok={}",
                    requested.elapsed().as_millis(),
                    result.is_ok()
                );
            });
        }
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
    if measure {
        println!("longest-turn-ms: {}", longest_turn.as_millis());
    }
    anyhow::ensure!(
        !await_reduction || reduction_done.get(),
        "Reduction did not complete before the diagnostic deadline"
    );
    Ok(())
}
