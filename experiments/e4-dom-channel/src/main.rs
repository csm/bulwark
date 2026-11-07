//! E4 measures Servo's public script and resource interception channels.
use anyhow::{Result, bail, ensure};
use servo::{
    EventLoopWaker, JSValue, LoadStatus, RenderingContext, Servo, ServoBuilder,
    SoftwareRenderingContext, UserContentManager, UserScript, WebResourceLoad, WebResourceResponse,
    WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const ORIGIN: &str = "https://bulwark.invalid";

#[derive(Clone)]
struct Waker(thread::Thread);
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {
        self.0.unpark();
    }
}

#[derive(Default)]
struct Delegate {
    ready: Cell<bool>,
    messages: RefCell<Vec<(String, String)>>,
}
impl WebViewDelegate for Delegate {
    fn notify_load_status_changed(&self, _: WebView, status: LoadStatus) {
        self.ready.set(status == LoadStatus::Complete);
    }
    fn notify_new_frame_ready(&self, view: WebView) {
        view.paint();
    }
    fn load_web_resource(&self, _: WebView, load: WebResourceLoad) {
        let url = load.request().url.clone();
        // Every fixture request is answered locally. No server or DNS is needed.
        let body = if url.origin().ascii_serialization() != ORIGIN {
            load.intercept(WebResourceResponse::new(url)).cancel();
            return;
        } else if url.path() == "/fixture" {
            let mut html = String::from("<!doctype html><meta charset=utf-8><div id=fixture>");
            for index in 0..5000 {
                html.push_str(&format!("<span id=n{index}>Node {index}</span>"));
            }
            html.push_str("</div>");
            html
        } else if let Some(id) = url.path().strip_prefix("/bridge/") {
            let value = url
                .query_pairs()
                .find_map(|(key, value)| (key == "value").then(|| value.into_owned()))
                .unwrap_or_default();
            self.messages.borrow_mut().push((id.into(), value));
            "ack".into()
        } else {
            load.intercept(WebResourceResponse::new(url)).cancel();
            return;
        };
        let mut response = WebResourceResponse::new(url);
        response
            .headers
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        let mut intercepted = load.intercept(response);
        intercepted.send_body_data(body.into_bytes());
        intercepted.finish();
    }
}

fn drive_until(servo: &Servo, deadline: Instant, mut complete: impl FnMut() -> bool) -> Result<()> {
    loop {
        ensure!(Instant::now() < deadline, "E4 process deadline exceeded");
        servo.spin_event_loop();
        if complete() {
            return Ok(());
        }
        thread::park_timeout(Duration::from_millis(1));
    }
}

fn evaluate(servo: &Servo, view: &WebView, deadline: Instant, script: &str) -> Result<String> {
    let (sender, receiver) = mpsc::channel();
    view.evaluate_javascript(script, move |result| {
        let _ = sender.send(result);
    });
    let mut result = None;
    let mut disconnected = false;
    drive_until(servo, deadline, || {
        match receiver.try_recv() {
            Ok(value) => result = Some(value),
            Err(mpsc::TryRecvError::Disconnected) => {
                disconnected = true;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        result.is_some() || disconnected
    })?;
    ensure!(!disconnected, "JavaScript callback dropped");
    match result.unwrap() {
        Ok(JSValue::String(text)) => Ok(text),
        other => bail!("Expected JavaScript string, got {other:?}"),
    }
}

fn measure(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    script: &str,
    count: usize,
    check: impl Fn(&str) -> Result<()>,
) -> Result<Vec<Duration>> {
    for _ in 0..5 {
        check(&evaluate(servo, view, deadline, script)?)?;
    }
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        let started = Instant::now();
        let value = evaluate(servo, view, deadline, script)?;
        samples.push(started.elapsed());
        check(&value)?;
    }
    samples.sort();
    Ok(samples)
}

fn report(label: &str, samples: &[Duration]) {
    println!(
        "E4 {label}: samples={}, p50={:?}, p95={:?}, max={:?}",
        samples.len(),
        samples[samples.len() / 2],
        samples[(samples.len() * 95 / 100).min(samples.len() - 1)],
        samples.last().unwrap()
    );
}

fn main() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let context = Rc::new(
        SoftwareRenderingContext::new((640, 480).into())
            .map_err(|error| anyhow::anyhow!("Software context: {error:?}"))?,
    );
    context
        .make_current()
        .map_err(|error| anyhow::anyhow!("GL context: {error:?}"))?;
    let servo = ServoBuilder::default()
        .event_loop_waker(Box::new(Waker(thread::current())))
        .build();
    let manager = Rc::new(UserContentManager::new(&servo));
    manager.add_script(Rc::new(UserScript::new(
        include_str!("../agent.js").into(),
        None,
    )));
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/fixture"))?)
        .user_content_manager(manager)
        .delegate(delegate.clone())
        .build();
    view.show();
    drive_until(&servo, deadline, || delegate.ready.get())?;

    let trivial = measure(
        &servo,
        &view,
        deadline,
        "bulwarkProbe.trivial()",
        100,
        |value| {
            ensure!(
                value == "bulwark-e4",
                "Injected agent returned incorrect value"
            );
            Ok(())
        },
    )?;
    let subtree = measure(
        &servo,
        &view,
        deadline,
        "bulwarkProbe.subtree()",
        30,
        |value| {
            let nodes: Vec<serde_json::Value> = serde_json::from_str(value)?;
            ensure!(nodes.len() == 5000, "Wrong subtree length");
            for (index, node) in nodes.iter().enumerate() {
                ensure!(
                    node["tag"] == "SPAN"
                        && node["id"] == format!("n{index}")
                        && node["text"] == format!("Node {index}"),
                    "Wrong node at {index}"
                );
            }
            Ok(())
        },
    )?;
    let payload = evaluate(&servo, &view, deadline, "bulwarkProbe.subtree()")?;
    // evaluate_javascript does not await Promises. Poll the page's explicit completion marker.
    evaluate(
        &servo,
        &view,
        deadline,
        "globalThis.bridgeReply = ''; bulwarkProbe.send(7, 'page → host & reply').then(x => bridgeReply = x); 'sent'",
    )?;
    loop {
        if evaluate(&servo, &view, deadline, "globalThis.bridgeReply")? == "ack" {
            break;
        }
    }
    ensure!(
        *delegate.messages.borrow() == [("7".into(), "page → host & reply".into())],
        "Resource interception lost or changed the page message"
    );
    report("trivial", &trivial);
    report("subtree_5000", &subtree);
    println!(
        "E4: subtree_bytes={}, intercepted_messages=1, reply=ack",
        payload.len()
    );
    ensure!(
        trivial[trivial.len() / 2] < Duration::from_millis(5),
        "Trivial p50 exceeds E4's 5 ms target"
    );
    Ok(())
}
