//! P3.1 mechanism probes. These intentionally do not install a privileged bridge.
use super::{ORIGIN, Waker, drive_until, evaluate};
use anyhow::{Result, ensure};
use servo::protocol_handler::{
    DoneChannel, FetchContext, ProtocolHandler, ProtocolRegistry, Request, ResourceFetchTiming,
    Response, ResponseBody,
};
use servo::{
    LoadStatus, NavigationRequest, RenderingContext, Servo, ServoBuilder, SoftwareRenderingContext,
    WebResourceLoad, WebResourceResponse, WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::{Cell, RefCell},
    future::{Future, ready},
    pin::Pin,
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct ProtocolSource {
    pipeline: String,
    view: String,
    origin: String,
    nested: Option<bool>,
}
struct DashboardProtocol(Arc<Mutex<Vec<ProtocolSource>>>);
impl ProtocolHandler for DashboardProtocol {
    fn load<'a>(
        &'a self,
        request: &'a mut Request,
        _: &mut DoneChannel,
        _: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send + 'a>> {
        if request.current_url().path() == "/message" {
            self.0.lock().unwrap().push(ProtocolSource {
                pipeline: format!("{:?}", request.pipeline_id),
                view: format!("{:?}", request.target_webview_id),
                origin: format!("{:?}", request.client.as_ref().map(|client| &client.origin)),
                nested: request
                    .client
                    .as_ref()
                    .map(|client| client.is_nested_browsing_context),
            });
        }
        let mut response = Response::new(
            request.current_url(),
            ResourceFetchTiming::new(request.timing_type()),
        );
        response
            .headers
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        response.headers.insert(
            "content-security-policy",
            "default-src 'none'".parse().unwrap(),
        );
        *response.body.lock() = ResponseBody::Done(
            b"<!doctype html><h1>Local dashboard</h1><p>Unprivileged protocol fixture</p>".to_vec(),
        );
        Box::pin(ready(response))
    }
    fn is_fetchable(&self) -> bool {
        true
    }
    fn is_secure(&self) -> bool {
        true
    }
    // The default empty privileged_paths deliberately grants no navigator.servo API.
}

#[derive(Default)]
struct Delegate {
    loads: Cell<usize>,
    navigation: RefCell<Vec<NavigationRequest>>,
    resources: RefCell<Vec<WebResourceLoad>>,
    // Public request metadata for equal fetches from the top document and an iframe.
    messages: RefCell<Vec<String>>,
}

fn respond(load: WebResourceLoad, body: &str) {
    let mut response = WebResourceResponse::new(load.request().url.clone());
    response
        .headers
        .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
    let mut intercepted = load.intercept(response);
    intercepted.send_body_data(body.as_bytes().to_vec());
    intercepted.finish();
}

impl WebViewDelegate for Delegate {
    fn notify_load_status_changed(&self, _: WebView, status: LoadStatus) {
        if status == LoadStatus::Complete {
            self.loads.set(self.loads.get() + 1);
        }
    }
    fn notify_new_frame_ready(&self, view: WebView) {
        view.paint();
    }
    fn request_navigation(&self, _: WebView, request: NavigationRequest) {
        if ["/next", "/denied"].contains(&request.url.path()) {
            self.navigation.borrow_mut().push(request);
        } else {
            request.allow();
        }
    }
    fn load_web_resource(&self, _: WebView, load: WebResourceLoad) {
        let url = load.request().url.clone();
        if url.scheme() == "bulwark" {
            return; // The registered local protocol supplies the response.
        }
        if (url.origin().ascii_serialization() == "https://remote.bulwark.invalid"
            && url.path() == "/remote-frame")
            || (url.origin().ascii_serialization() == ORIGIN && url.path() == "/sandbox-frame")
        {
            respond(
                load,
                "<!doctype html><script>\
                 fetch('bulwark://phase3/message?claimed=top',{mode:'no-cors',referrerPolicy:'no-referrer'})\
                   .then(()=>parent.postMessage(location.pathname,'*'));</script>",
            );
            return;
        }
        if url.origin().ascii_serialization() != ORIGIN {
            load.intercept(WebResourceResponse::new(url)).cancel();
            return;
        }
        match url.path() {
            "/fixture" | "/next" => respond(
                load,
                &format!(
                    "<!doctype html><meta charset=utf-8><h1>{}</h1>\
                     <script>window.ticks=0;setInterval(()=>ticks++,5);</script>",
                    url.path()
                ),
            ),
            "/frame" => respond(
                load,
                "<!doctype html><script>\
                 fetch('/message?claimed=top',{referrerPolicy:'no-referrer'})\
                   .then(r=>r.text()).then(x=>parent.frameReply=x);</script>",
            ),
            "/protocol-frame" => respond(
                load,
                "<!doctype html><script>\
                 fetch('bulwark://phase3/message?claimed=top',{mode:'no-cors',referrerPolicy:'no-referrer'})\
                   .then(()=>parent.protocolFrame='received');</script>",
            ),
            "/message" => {
                self.messages
                    .borrow_mut()
                    .push(format!("{:?}", load.request()));
                respond(load, "ack");
            }
            "/delayed" | "/cancelled" => self.resources.borrow_mut().push(load),
            _ => load.intercept(WebResourceResponse::new(url)).cancel(),
        }
    }
}

fn turn_for(servo: &Servo, deadline: Instant, duration: Duration) -> Result<()> {
    let end = Instant::now() + duration;
    drive_until(servo, deadline, || Instant::now() >= end)
}

fn until_text(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    script: &str,
    expected: &str,
) -> Result<()> {
    loop {
        if evaluate(servo, view, deadline, script)? == expected {
            return Ok(());
        }
        turn_for(servo, deadline, Duration::from_millis(2))?;
    }
}

pub(super) fn run() -> Result<()> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let context = Rc::new(
        SoftwareRenderingContext::new((640, 480).into())
            .map_err(|error| anyhow::anyhow!("Software context: {error:?}"))?,
    );
    context
        .make_current()
        .map_err(|error| anyhow::anyhow!("GL context: {error:?}"))?;
    let mut protocols = ProtocolRegistry::default();
    let protocol_sources = Arc::new(Mutex::new(Vec::new()));
    protocols
        .register("bulwark", DashboardProtocol(protocol_sources.clone()))
        .map_err(|error| anyhow::anyhow!("Protocol registration: {error:?}"))?;
    let servo = ServoBuilder::default()
        .protocol_registry(protocols)
        .event_loop_waker(Box::new(Waker(thread::current())))
        .build();
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/fixture"))?)
        .delegate(delegate.clone())
        .build();
    view.show();
    drive_until(&servo, deadline, || delegate.loads.get() > 0)?;

    // A responder can leave the callback without allowing the pending navigation.
    evaluate(&servo, &view, deadline, "location.href='/next';'requested'")?;
    drive_until(&servo, deadline, || {
        !delegate.navigation.borrow().is_empty()
    })?;
    let before: u64 = evaluate(&servo, &view, deadline, "String(ticks)")?.parse()?;
    turn_for(&servo, deadline, Duration::from_millis(50))?;
    let after: u64 = evaluate(&servo, &view, deadline, "String(ticks)")?.parse()?;
    ensure!(
        after > before,
        "Page timers stopped during deferred navigation"
    );
    ensure!(
        evaluate(
            &servo,
            &view,
            deadline,
            "document.querySelector('h1').textContent"
        )? == "/fixture",
        "Navigation ran before its deferred decision"
    );
    delegate.navigation.borrow_mut().remove(0).allow();
    drive_until(&servo, deadline, || delegate.loads.get() >= 2)?;
    ensure!(
        evaluate(
            &servo,
            &view,
            deadline,
            "document.querySelector('h1').textContent"
        )? == "/next",
        "Deferred allow did not navigate"
    );
    evaluate(
        &servo,
        &view,
        deadline,
        "location.href='/denied';'requested'",
    )?;
    drive_until(&servo, deadline, || {
        !delegate.navigation.borrow().is_empty()
    })?;
    delegate.navigation.borrow_mut().remove(0).deny();
    turn_for(&servo, deadline, Duration::from_millis(50))?;
    ensure!(
        evaluate(&servo, &view, deadline, "location.pathname")? == "/next",
        "Deferred denial changed the document"
    );
    println!("P3.1 Servo: deferred navigation allow/deny passed; page timers progressed");

    evaluate(
        &servo,
        &view,
        deadline,
        "window.reply='pending';fetch('/delayed').then(r=>r.text()).then(x=>reply=x);'sent'",
    )?;
    drive_until(&servo, deadline, || !delegate.resources.borrow().is_empty())?;
    turn_for(&servo, deadline, Duration::from_millis(50))?;
    ensure!(
        evaluate(&servo, &view, deadline, "reply")? == "pending",
        "Resource completed before its deferred response"
    );
    respond(delegate.resources.borrow_mut().remove(0), "deferred-ack");
    until_text(&servo, &view, deadline, "reply", "deferred-ack")?;
    evaluate(
        &servo,
        &view,
        deadline,
        "window.reply='pending';fetch('/cancelled').catch(()=>reply='cancelled');'sent'",
    )?;
    drive_until(&servo, deadline, || !delegate.resources.borrow().is_empty())?;
    let load = delegate.resources.borrow_mut().remove(0);
    load.intercept(WebResourceResponse::new(load_url("/cancelled")?))
        .cancel();
    until_text(&servo, &view, deadline, "reply", "cancelled")?;
    println!("P3.1 Servo: deferred resource response/cancel passed");

    // Both fetches deliberately claim to be from the top document. The entire public
    // WebResourceRequest is equal: neither request identifies its source frame.
    evaluate(
        &servo,
        &view,
        deadline,
        "window.topReply='';window.frameReply='';\
         fetch('/message?claimed=top',{referrerPolicy:'no-referrer'})\
           .then(r=>r.text()).then(x=>topReply=x);\
         let frame=document.createElement('iframe');frame.src='/frame';\
         document.body.appendChild(frame);'sent'",
    )?;
    until_text(
        &servo,
        &view,
        deadline,
        "topReply+':'+frameReply",
        "ack:ack",
    )?;
    let messages = delegate.messages.borrow();
    ensure!(
        messages.len() == 2,
        "Expected top-document and iframe messages"
    );
    ensure!(
        messages[0] == messages[1],
        "Public request metadata differed: {messages:?}"
    );
    println!("P3.1 Servo gap: top-document and iframe fetch metadata are identical");
    drop(messages);

    // The custom protocol receives richer metadata than WebResourceLoad. Determine
    // whether this public alternative can distinguish the source frame.
    evaluate(
        &servo,
        &view,
        deadline,
        "window.protocolTop='';window.protocolFrame='';window.protocolOther=[];\
         window.addEventListener('message',e=>protocolOther.push(e.data));\
         fetch('bulwark://phase3/message?claimed=top',{mode:'no-cors',referrerPolicy:'no-referrer'})\
           .then(()=>protocolTop='received');\
         let protocolFrameElement=document.createElement('iframe');\
         protocolFrameElement.src='/protocol-frame';document.body.appendChild(protocolFrameElement);'sent'",
    )?;
    until_text(
        &servo,
        &view,
        deadline,
        "protocolTop+':'+protocolFrame",
        "received:received",
    )?;
    {
        let sources = protocol_sources.lock().unwrap();
        ensure!(
            sources.len() == 2,
            "Expected two custom-protocol messages: {sources:?}"
        );
        ensure!(
            sources[0].pipeline != "None" && sources[1].pipeline != "None",
            "Missing protocol pipeline: {sources:?}"
        );
        ensure!(
            sources[0].pipeline != sources[1].pipeline,
            "Protocol did not distinguish source pipelines: {sources:?}"
        );
        ensure!(
            sources[0].origin == sources[1].origin,
            "Unexpected same-origin frame metadata: {sources:?}"
        );
        ensure!(
            sources.iter().any(|s| s.nested == Some(false))
                && sources.iter().any(|s| s.nested == Some(true)),
            "Protocol did not distinguish nesting: {sources:?}"
        );
    }
    evaluate(
        &servo,
        &view,
        deadline,
        "let remote=document.createElement('iframe');remote.src='https://remote.bulwark.invalid/remote-frame';\
         document.body.appendChild(remote);\
         let sandboxed=document.createElement('iframe');sandboxed.sandbox='allow-scripts';\
         sandboxed.src='/sandbox-frame';document.body.appendChild(sandboxed);'sent'",
    )?;
    until_text(
        &servo,
        &view,
        deadline,
        "String(protocolOther.includes('/remote-frame')&&protocolOther.includes('/sandbox-frame'))",
        "true",
    )?;
    {
        let sources = protocol_sources.lock().unwrap();
        ensure!(
            sources.len() == 4,
            "Expected four protocol sources: {sources:?}"
        );
        ensure!(
            sources
                .iter()
                .all(|s| s.view != "None" && s.view == sources[0].view),
            "Protocol lost the owning view: {sources:?}"
        );
        ensure!(
            sources
                .iter()
                .any(|s| s.origin.contains("remote.bulwark.invalid") && s.nested == Some(true)),
            "Remote-frame origin was not retained: {sources:?}"
        );
        ensure!(
            sources
                .iter()
                .any(|s| s.origin.contains("Opaque") && s.nested == Some(true)),
            "Sandboxed origin was not opaque: {sources:?}"
        );
        let pipelines: std::collections::BTreeSet<_> =
            sources.iter().map(|s| &s.pipeline).collect();
        ensure!(
            pipelines.len() == 4 && !pipelines.iter().any(|p| p.as_str() == "None"),
            "Protocol did not retain distinct source pipelines: {sources:?}"
        );
        println!(
            "P3.1 Servo candidate: custom protocol identifies top, same-origin, remote, and sandboxed sources: {sources:?}"
        );
    }

    evaluate(&servo, &view, deadline, include_str!("../phase3-agent.js"))?;
    evaluate(
        &servo,
        &view,
        deadline,
        "for(let i=0;i<500;i++)document.body.appendChild(document.createElement('span'));'mutated'",
    )?;
    until_text(
        &servo,
        &view,
        deadline,
        "String(phase3Agent.overflow)",
        "true",
    )?;
    let batch: serde_json::Value =
        serde_json::from_str(&evaluate(&servo, &view, deadline, "phase3Agent.drain()")?)?;
    ensure!(
        batch["records"].as_array().unwrap().len() == 64,
        "Wrong batch limit"
    );
    ensure!(batch["overflow"] == true, "Overflow was hidden");
    let empty: serde_json::Value =
        serde_json::from_str(&evaluate(&servo, &view, deadline, "phase3Agent.drain()")?)?;
    ensure!(
        empty["records"].as_array().unwrap().is_empty(),
        "Drain repeated records"
    );
    ensure!(empty["overflow"] == true, "Terminal overflow was lost");
    evaluate(
        &servo,
        &view,
        deadline,
        "phase3Agent={drain:()=>'{\"records\":[],\"overflow\":false}'};'replaced'",
    )?;
    ensure!(
        evaluate(&servo, &view, deadline, "phase3Agent.drain()")?
            == "{\"records\":[],\"overflow\":false}",
        "Agent replacement did not reproduce the shared-world limitation"
    );
    println!("P3.1 Servo: bounded observer drain passed; shared-world agent forgery reproduced");

    evaluate(&servo, &view, deadline, "window.retainedMarker='retained'")?;
    let dashboard_delegate = Rc::new(Delegate::default());
    let dashboard = WebViewBuilder::new(&servo, context)
        .url(url::Url::parse("bulwark://phase3/dashboard")?)
        .delegate(dashboard_delegate.clone())
        .build();
    view.hide();
    view.set_focused(false);
    dashboard.show();
    dashboard.set_focused(true);
    drive_until(&servo, deadline, || dashboard_delegate.loads.get() > 0)?;
    ensure!(
        evaluate(
            &servo,
            &dashboard,
            deadline,
            "document.querySelector('h1').textContent"
        )? == "Local dashboard",
        "Local custom protocol did not supply HTML"
    );
    ensure!(
        evaluate(&servo, &dashboard, deadline, "typeof navigator.servo")? == "undefined",
        "Protocol fixture unexpectedly received Servo internals authority"
    );
    let origin = evaluate(&servo, &dashboard, deadline, "location.origin")?;
    dashboard.hide();
    dashboard.set_focused(false);
    view.show();
    view.set_focused(true);
    ensure!(
        evaluate(&servo, &view, deadline, "retainedMarker")? == "retained",
        "Custom-view switching lost the retained web document"
    );
    println!(
        "P3.1 Servo: bulwark:// HTML and retained-view switching passed; origin={origin}; no Servo internals privilege"
    );
    println!("P3.1 Servo contracts passed; document identity and bridge authority remain unproven");
    Ok(())
}

fn load_url(path: &str) -> Result<url::Url> {
    Ok(url::Url::parse(&format!("{ORIGIN}{path}"))?)
}
