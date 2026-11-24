//! Isolated S1 prototype regressions. Requires the experimental Servo patch.
use super::{ORIGIN, Waker, drive_until, evaluate};
use anyhow::{Result, ensure};
use servo::{
    ConsoleLogLevel, CreateNewWebViewRequest, DocumentIdentity, DocumentJavaScriptResult,
    DocumentLifecycle, JSValue, JavaScriptEvaluationError, LoadStatus, Opts, RenderingContext,
    Servo, ServoBuilder, SoftwareRenderingContext, TraversalId, WebResourceLoad,
    WebResourceResponse, WebView, WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::{Cell, RefCell},
    io::{Read, Write},
    net::TcpListener,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

type Evaluation = Result<DocumentJavaScriptResult, JavaScriptEvaluationError>;

#[derive(Default)]
struct Delegate {
    loads: Cell<usize>,
    loopback_port: Cell<u16>,
    active: Cell<Option<DocumentIdentity>>,
    generation: Cell<u64>,
    allocations: Cell<usize>,
    events: RefCell<Vec<DocumentLifecycle>>,
    closed: Cell<bool>,
    released: Cell<bool>,
    traversals: RefCell<Vec<TraversalId>>,
    crashed: Cell<bool>,
    crash_reasons: RefCell<Vec<String>>,
    popups: RefCell<Vec<CreateNewWebViewRequest>>,
    // One-shot: panic after the host revokes local state for an inactive document.
    panic_on_revoke: Cell<bool>,
    // Page console messages order the interruption fixtures. They grant no host authority.
    console: RefCell<Vec<String>>,
}

const DELEGATE_PANIC: &str = "S1 probe delegate panic";
const CALLBACK_PANIC: &str = "S1 probe terminal callback panic";

#[derive(Clone, Copy)]
struct Ticket {
    document: DocumentIdentity,
    generation: u64,
    cancelled: bool,
}

impl Delegate {
    fn notify_document_lifecycle_without_view(&self, event: DocumentLifecycle) {
        if let DocumentLifecycle::Active(document) = event
            && self.active.replace(document) != document
        {
            self.generation
                .set(self.generation.get().checked_add(1).unwrap());
        }
        self.events.borrow_mut().push(event);
        // The host revokes before any fallible work. The engine must still deliver.
        if event == DocumentLifecycle::Active(None) && self.panic_on_revoke.replace(false) {
            panic!("{DELEGATE_PANIC}");
        }
    }

    fn ticket(&self, document: DocumentIdentity) -> Result<Ticket> {
        ensure!(
            self.active.get() == Some(document),
            "Admission before activation"
        );
        Ok(Ticket {
            document,
            generation: self.generation.get(),
            cancelled: false,
        })
    }

    // Model only the allocation boundary, not a production node registry. No page data
    // may update this state. Check both scope and activation before allocating a handle.
    fn deliver(&self, ticket: Ticket, result: Evaluation) -> bool {
        let Ok(result) = result else { return false };
        if ticket.cancelled
            || self.active.get() != Some(ticket.document)
            || self.generation.get() != ticket.generation
            || result.document != ticket.document
        {
            return false;
        }
        self.allocations.set(self.allocations.get() + 1);
        true
    }

    fn reject(&self, ticket: Ticket, result: Evaluation) -> Result<()> {
        let before = self.allocations.get();
        ensure!(
            !self.deliver(ticket, result),
            "Obsolete reply reached allocation"
        );
        ensure!(
            self.allocations.get() == before,
            "Rejection allocated a handle"
        );
        Ok(())
    }
}
impl WebViewDelegate for Delegate {
    fn notify_document_scope_released(&self) {
        self.released.set(true);
        self.notify_document_lifecycle_without_view(DocumentLifecycle::Active(None));
    }
    fn request_create_new(&self, _: WebView, request: CreateNewWebViewRequest) {
        self.popups.borrow_mut().push(request);
    }
    fn notify_document_lifecycle(&self, _: WebView, event: DocumentLifecycle) {
        self.notify_document_lifecycle_without_view(event);
    }
    fn notify_traversal_complete(&self, _: WebView, id: TraversalId) {
        self.traversals.borrow_mut().push(id);
    }
    fn notify_closed(&self, _: WebView) {
        self.closed.set(true);
    }
    fn notify_crashed(&self, _: WebView, reason: String, _: Option<String>) {
        self.crash_reasons.borrow_mut().push(reason);
        self.crashed.set(true);
    }
    fn notify_load_status_changed(&self, _: WebView, status: LoadStatus) {
        if status == LoadStatus::Complete {
            self.loads.set(self.loads.get() + 1);
        }
    }
    fn notify_new_frame_ready(&self, view: WebView) {
        view.paint();
    }
    fn show_console_message(&self, _: WebView, _: ConsoleLogLevel, message: String) {
        self.console.borrow_mut().push(message);
    }
    fn load_web_resource(&self, _: WebView, load: WebResourceLoad) {
        let url = load.request().url.clone();
        if url.host_str() == Some("127.0.0.1") && url.port() == Some(self.loopback_port.get()) {
            // Use the real HTTP loader for the redirect; interception bypasses that path.
            return;
        }
        if url.scheme() == "s1-failure" {
            // The default loader produces UnsupportedScheme without network access.
            return;
        }
        if url.origin().ascii_serialization() != ORIGIN {
            load.intercept(WebResourceResponse::new(url)).cancel();
            return;
        }
        let mut response = WebResourceResponse::new(url.clone());
        response
            .headers
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        let mut intercepted = load.intercept(response);
        intercepted.send_body_data(
            format!(
                "<!doctype html><h1>{}</h1><script>window.marker={:?};window.mutations=0</script>{}",
                url.path(),
                url.path(),
                if url.path() == "/other" { "<iframe src='/child'></iframe>" } else { "" }
            )
            .into_bytes(),
        );
        intercepted.finish();
    }
}

fn receive<T>(servo: &Servo, deadline: Instant, receiver: Receiver<T>) -> Result<T> {
    let mut reply = None;
    let mut disconnected = false;
    drive_until(servo, deadline, || match receiver.try_recv() {
        Ok(result) => {
            reply = Some(result);
            true
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            disconnected = true;
            true
        }
        Err(mpsc::TryRecvError::Empty) => false,
    })?;
    ensure!(!disconnected, "Document callback dropped");
    Ok(reply.unwrap())
}

fn identity(servo: &Servo, view: &WebView, deadline: Instant) -> Result<DocumentIdentity> {
    let (sender, receiver) = mpsc::channel();
    view.request_document_identity(move |result| {
        let _ = sender.send(result);
    });
    receive(servo, deadline, receiver)?.map_err(|error| anyhow::anyhow!("Identity: {error:?}"))
}

fn queue(view: &WebView, document: DocumentIdentity, script: &str) -> Receiver<Evaluation> {
    let (sender, receiver) = mpsc::channel();
    view.evaluate_javascript_in_document(document, script, move |result| {
        let _ = sender.send(result);
    });
    receiver
}

fn scoped(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    document: DocumentIdentity,
    script: &str,
) -> Result<Evaluation> {
    receive(servo, deadline, queue(view, document, script))
}

fn stale(result: Evaluation) -> Result<()> {
    ensure!(
        matches!(result, Err(JavaScriptEvaluationError::StaleDocument)),
        "Expected stale-document denial, got {result:?}"
    );
    Ok(())
}

fn text(result: Evaluation, document: DocumentIdentity, expected: &str) -> Result<()> {
    let result = result.map_err(|error| anyhow::anyhow!("Scoped evaluation: {error:?}"))?;
    ensure!(
        result.document == document,
        "Result changed document identity"
    );
    ensure!(
        result.value == JSValue::String(expected.into()),
        "Unexpected result: {result:?}"
    );
    Ok(())
}

fn load(servo: &Servo, view: &WebView, deadline: Instant, path: &str) -> Result<()> {
    view.load(url::Url::parse(&format!("{ORIGIN}{path}"))?);
    wait_marker(servo, view, deadline, path)
}

fn wait_marker(servo: &Servo, view: &WebView, deadline: Instant, path: &str) -> Result<()> {
    // A fixture marker establishes test ordering only; it grants no host authority.
    loop {
        let (sender, receiver) = mpsc::channel();
        view.evaluate_javascript("String(window.marker)", move |result| {
            let _ = sender.send(result);
        });
        match receive(servo, deadline, receiver)? {
            Ok(JSValue::String(marker)) if marker == path => return Ok(()),
            Ok(JSValue::String(_))
            | Err(JavaScriptEvaluationError::StaleDocument)
            | Err(JavaScriptEvaluationError::WebViewNotReady)
            | Err(JavaScriptEvaluationError::DocumentNotFound) => {}
            other => anyhow::bail!("Unexpected navigation marker result: {other:?}"),
        }
        thread::park_timeout(Duration::from_millis(1));
    }
}

fn redirect_fixture() -> Result<(u16, thread::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        for expected in ["/redirect", "/redirect-final"] {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        ensure!(
                            Instant::now() < deadline,
                            "Loopback fixture deadline exceeded"
                        );
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            // Accepted sockets can inherit the listener's nonblocking mode on macOS.
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte)?;
                request.push(byte[0]);
                ensure!(request.len() <= 16384, "Oversized loopback request");
            }
            ensure!(
                String::from_utf8_lossy(&request)
                    .starts_with(&format!("GET {expected} HTTP/1.1\r\n")),
                "Unexpected loopback request"
            );
            eprintln!("Loopback received {expected}");
            if expected == "/redirect" {
                stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /redirect-final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
            } else {
                let body = "<!doctype html><h1>Redirected</h1><script>window.marker='/redirect-final'</script>";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
            }
        }
        Ok(())
    });
    Ok((port, worker))
}

// Keep captures observable independently of engine replies. Every group mixes scoped
// and unscoped requests; no callback may retain its captures after terminal delivery.
struct CleanupAudit {
    released: std::rc::Weak<()>,
    calls: Rc<Cell<usize>>,
    replies: Receiver<(Option<DocumentIdentity>, bool)>,
}

impl CleanupAudit {
    fn queue(
        view: &WebView,
        document: Option<DocumentIdentity>,
        delegate: &Rc<Delegate>,
        retain_view: bool,
    ) -> Self {
        Self::queue_with_panics(view, document, delegate, retain_view, false)
    }

    fn queue_with_panics(
        view: &WebView,
        document: Option<DocumentIdentity>,
        delegate: &Rc<Delegate>,
        retain_view: bool,
        panic_after_delivery: bool,
    ) -> Self {
        let capture = Rc::new(());
        let released = Rc::downgrade(&capture);
        let calls = Rc::new(Cell::new(0));
        let (sender, replies) = mpsc::channel();
        for index in 0..8 {
            let capture = capture.clone();
            let calls = calls.clone();
            let sender = sender.clone();
            let delegate = delegate.clone();
            // Exercise the callback -> view -> Servo ownership cycle during explicit shutdown.
            let retained_view = retain_view.then(|| view.clone());
            let callback = move |stale| {
                drop(capture);
                drop(retained_view);
                calls.set(calls.get() + 1);
                let _ = sender.send((delegate.active.get(), stale));
                if panic_after_delivery {
                    panic!("{CALLBACK_PANIC}");
                }
            };
            if let Some(document) = document.filter(|_| index % 2 == 0) {
                view.evaluate_javascript_in_document(document, "'pending'", move |result| {
                    callback(matches!(
                        result,
                        Err(JavaScriptEvaluationError::StaleDocument)
                    ));
                });
            } else {
                view.evaluate_javascript("'pending'", move |result| {
                    callback(matches!(
                        result,
                        Err(JavaScriptEvaluationError::StaleDocument)
                    ));
                });
            }
        }
        Self {
            released,
            calls,
            replies,
        }
    }

    fn check(&self) -> Result<()> {
        ensure!(self.calls.get() == 8, "Expected eight terminal callbacks");
        for _ in 0..8 {
            let (active, stale) = self.replies.try_recv()?;
            ensure!(
                active.is_none() && stale,
                "Cleanup callback preceded revocation or returned success"
            );
        }
        ensure!(
            self.replies.try_recv().is_err(),
            "Duplicate cleanup callback"
        );
        ensure!(
            self.released.upgrade().is_none(),
            "Cleanup retained callback captures"
        );
        Ok(())
    }
}

// Wait until the running script logs entry. This message arrives while the script runs.
fn wait_console(servo: &Servo, delegate: &Delegate, deadline: Instant, marker: &str) -> Result<()> {
    drive_until(servo, deadline, || {
        delegate
            .console
            .borrow()
            .iter()
            .any(|message| message == marker)
    })
}

fn expect_panic(expected: &str, action: impl FnOnce()) -> Result<()> {
    let payload = catch_unwind(AssertUnwindSafe(action))
        .expect_err("Panic did not propagate to the embedder");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());
    ensure!(
        message == Some(expected),
        "Unexpected panic payload: {message:?}"
    );
    Ok(())
}

fn expect_callback_panic(action: impl FnOnce()) -> Result<()> {
    expect_panic(CALLBACK_PANIC, action)
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
    let servo = ServoBuilder::default()
        .opts(Opts {
            hard_fail: false,
            ..Opts::default()
        })
        .event_loop_waker(Box::new(Waker(thread::current())))
        .build();
    // Servo's own crash tests use this public logging path to report thread panics.
    servo.setup_logging();
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic");
        log::error!("{message}");
    }));
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse("about:blank")?)
        .delegate(delegate.clone())
        .build();
    view.show();
    drive_until(&servo, deadline, || delegate.loads.get() > 0)?;
    let blank = identity(&servo, &view, deadline)?;
    load(&servo, &view, deadline, "/a")?;
    let a = identity(&servo, &view, deadline)?;
    ensure!(
        a != blank,
        "Initial about:blank identity survived replacement"
    );
    text(
        scoped(&servo, &view, deadline, a, "String(++mutations)")?,
        a,
        "1",
    )?;
    ensure!(
        delegate.active.get() == Some(a),
        "Missing activation notification"
    );
    drive_until(&servo, deadline, || {
        let events = delegate.events.borrow();
        events.contains(&DocumentLifecycle::DomReady(a))
            && events.contains(&DocumentLifecycle::LoadComplete(a))
    })?;
    let ticket = delegate.ticket(a)?;
    ensure!(
        delegate.deliver(ticket, scoped(&servo, &view, deadline, a, "'current'")?),
        "Current reply rejected"
    );
    let (sender, reentrant) = mpsc::channel();
    let callback_view = view.clone();
    view.evaluate_javascript_in_document(a, "'first'", move |first| {
        if first.is_err() {
            let _ = sender.send(first);
            return;
        }
        callback_view.evaluate_javascript_in_document(a, "'reentrant'", move |result| {
            let _ = sender.send(result);
        });
    });
    text(receive(&servo, deadline, reentrant)?, a, "reentrant")?;
    println!("S1 initial-blank-and-tagged-result: PASS");
    println!("S1 lifecycle-readiness-and-current-delivery: PASS");

    let same_generation = delegate.generation.get();
    evaluate(
        &servo,
        &view,
        deadline,
        "location.hash='fragment';'changed'",
    )?;
    ensure!(
        identity(&servo, &view, deadline)? == a,
        "Fragment replaced document identity"
    );
    evaluate(
        &servo,
        &view,
        deadline,
        "history.pushState({},'', '/a?state=1');'changed'",
    )?;
    ensure!(
        identity(&servo, &view, deadline)? == a,
        "pushState replaced document identity"
    );
    ensure!(
        delegate.generation.get() == same_generation,
        "Same-document navigation revoked activation"
    );
    println!("S1 same-document-navigation: PASS");

    // A script-created top-level initial document must have its own authenticated scope.
    let popup_open = queue(&view, a, "window.open('about:blank');'opened'");
    drive_until(&servo, deadline, || !delegate.popups.borrow().is_empty())?;
    let popup_delegate = Rc::new(Delegate::default());
    let popup = delegate
        .popups
        .borrow_mut()
        .pop()
        .unwrap()
        .builder(context.clone())
        .delegate(popup_delegate.clone())
        .build();
    text(receive(&servo, deadline, popup_open)?, a, "opened")?;
    let popup_blank = identity(&servo, &popup, deadline)?;
    ensure!(
        popup_blank != a,
        "Script-created initial document reused the opener scope"
    );
    ensure!(
        popup_delegate.active.get() == Some(popup_blank),
        "Initial popup activation missing"
    );
    let popup_ticket = popup_delegate.ticket(popup_blank)?;
    let popup_reply = scoped(&servo, &popup, deadline, popup_blank, "'initial'")?;
    ensure!(popup_reply.is_ok(), "Initial popup control failed");
    stale(scoped(
        &servo,
        &popup,
        deadline,
        popup_blank,
        "document.open();document.write('<h1>Script-created</h1>');document.close();'reset'",
    )?)?;
    let popup_reset = identity(&servo, &popup, deadline)?;
    ensure!(
        popup_reset != popup_blank,
        "Script-created reset preserved the initial scope"
    );
    popup_delegate.reject(popup_ticket, popup_reply)?;
    stale(scoped(&servo, &popup, deadline, popup_blank, "'obsolete'")?)?;
    ensure!(
        identity(&servo, &view, deadline)? == a,
        "Popup reset changed opener scope"
    );
    popup.close_for_document_scope_probe();
    drive_until(&servo, deadline, || popup_delegate.closed.get())?;
    println!("S1 script-created-initial-document: PASS");

    // Model a native command admitted for A but held in the host queue until B is active.
    let held_document = a;
    let held_ticket = delegate.ticket(a)?;
    let held_script = "document.querySelector('h1').textContent='WRONG';String(++mutations)";
    load(&servo, &view, deadline, "/b")?;
    let b = identity(&servo, &view, deadline)?;
    ensure!(a != b, "Replacement retained document identity");
    let held_reply = receive(&servo, deadline, queue(&view, held_document, held_script))?;
    stale(held_reply.clone())?;
    delegate.reject(held_ticket, held_reply)?;
    ensure!(
        evaluate(
            &servo,
            &view,
            deadline,
            "document.querySelector('h1').textContent+':'+mutations"
        )? == "/b:0",
        "Stale mutation ran in replacement document"
    );
    // The original public API is the negative control: it selects B at execution time.
    ensure!(
        evaluate(&servo, &view, deadline, held_script)? == "1",
        "Unscoped control did not execute"
    );
    println!("S1 held-A-after-B-and-unscoped-control: PASS");

    // FIFO script messages give a deterministic execution-boundary race: the reset executes
    // after both requests are queued, before the second expected-identity check.
    let reset = queue(
        &view,
        b,
        "document.open();document.write('<h1>Reset</h1>');document.close();'reset'",
    );
    let mutation = queue(
        &view,
        b,
        "document.querySelector('h1').textContent='WRONG';'mutated'",
    );
    stale(receive(&servo, deadline, reset)?)?;
    stale(receive(&servo, deadline, mutation)?)?;
    let reset_id = identity(&servo, &view, deadline)?;
    ensure!(reset_id != b, "document.open retained document incarnation");
    ensure!(
        reset_id.pipeline_id() == b.pipeline_id(),
        "Reset fixture did not preserve the pipeline"
    );
    text(
        scoped(
            &servo,
            &view,
            deadline,
            reset_id,
            "document.querySelector('h1').textContent",
        )?,
        reset_id,
        "Reset",
    )?;
    println!("S1 queued-reset-race-and-pipeline-collision: PASS");

    // WebDriver-style serialization invokes getters. A getter must not return a success
    // tagged with the document from before its reset.
    stale(scoped(
        &servo,
        &view,
        deadline,
        reset_id,
        "({get value(){document.open();document.write('<h1>Getter reset</h1>');document.close();return 'forged'}})",
    )?)?;
    let getter_id = identity(&servo, &view, deadline)?;
    ensure!(getter_id != reset_id, "Getter reset retained identity");
    println!("S1 serialization-reset: PASS");

    // Hold a successful callback across an engine-authenticated reset notification.
    let delayed_ticket = delegate.ticket(getter_id)?;
    let delayed = queue(&view, getter_id, "'old-document-data'");
    stale(scoped(
        &servo,
        &view,
        deadline,
        getter_id,
        "document.open();document.write('<h1>After delivery</h1>');document.close();'reset'",
    )?)?;
    let after_delivery = identity(&servo, &view, deadline)?;
    let obsolete = receive(&servo, deadline, delayed)?
        .map_err(|error| anyhow::anyhow!("Expected a retained success: {error:?}"))?;
    ensure!(
        obsolete.document == getter_id && obsolete.document != after_delivery,
        "Delayed result lost its obsolete identity"
    );
    ensure!(
        delegate.active.get() == Some(after_delivery),
        "Reset notification missing"
    );
    delegate.reject(delayed_ticket, Ok(obsolete))?;
    println!("S1 delayed-host-delivery: PASS (revoked before allocation)");

    let mut cancelled = delegate.ticket(after_delivery)?;
    let cancelled_reply = scoped(&servo, &view, deadline, after_delivery, "'cancelled'")?;
    ensure!(
        cancelled_reply.is_ok(),
        "Cancellation control did not succeed"
    );
    cancelled.cancelled = true;
    delegate.reject(cancelled, cancelled_reply)?;
    println!("S1 host-cancellation: PASS");

    // Hold the receiving script thread. Cancellation must prevent this queued mutation,
    // not merely suppress its reply. The host never pumps results inside the barrier.
    let cancel_delegate = Rc::new(Delegate::default());
    let cancel_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/cancellation"))?)
        .delegate(cancel_delegate.clone())
        .build();
    drive_until(&servo, deadline, || cancel_delegate.loads.get() > 0)?;
    let cancel_document = identity(&servo, &cancel_view, deadline)?;
    let cancel_ticket = cancel_delegate.ticket(cancel_document)?;
    let cancellation_calls = Rc::new(Cell::new(0));
    let capture = Rc::new(());
    let released_capture = Rc::downgrade(&capture);
    let (sender, cancellation_reply) = mpsc::channel();
    let (sender_survivor, cancel_survivor) = mpsc::channel();
    let survivor_view = view.clone();
    let calls = cancellation_calls.clone();
    let mut cancelled_id = None;
    cancel_view.with_paused_script_for_document_scope_probe(|| {
        let id = cancel_view
            .evaluate_cancellable_javascript_in_document(
                cancel_document,
                "window.mutations++;'must-not-run'",
                move |result| {
                    let _capture = capture;
                    calls.set(calls.get() + 1);
                    let _ = sender.send(result);
                    // Reentry must not hold the evaluator's RefCell borrow.
                    survivor_view.evaluate_javascript("'cancel-survivor'", move |result| {
                        let _ = sender_survivor.send(result);
                    });
                },
            )
            .unwrap();
        ensure!(
            !view.cancel_javascript_evaluation(id),
            "A foreign view cancelled the request"
        );
        ensure!(
            cancel_view.cancel_javascript_evaluation(id),
            "Cancellation missed pending work"
        );
        ensure!(
            !cancel_view.cancel_javascript_evaluation(id),
            "Repeated cancellation delivered twice"
        );
        cancelled_id = Some(id);
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        cancellation_calls.get() == 1,
        "Cancellation missed terminal delivery"
    );
    ensure!(
        released_capture.upgrade().is_none(),
        "Cancellation retained captures"
    );
    let cancelled_result = cancellation_reply.try_recv()?;
    ensure!(
        matches!(cancelled_result, Err(JavaScriptEvaluationError::Cancelled)),
        "Cancellation did not deliver Cancelled: {cancelled_result:?}"
    );
    cancel_delegate.reject(cancel_ticket, cancelled_result)?;
    ensure!(
        matches!(receive(&servo, deadline, cancel_survivor)?, Ok(JSValue::String(value)) if value == "cancel-survivor"),
        "Cancellation blocked reentrant evaluation"
    );
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "0",
    )?;
    ensure!(
        cancel_delegate.active.get() == Some(cancel_document)
            && cancel_delegate.generation.get() == cancel_ticket.generation
            && cancel_view.pending_evaluations_for_document_scope_probe() == 0
            && cancellation_calls.get() == 1
            && !cancel_view.cancel_javascript_evaluation(cancelled_id.unwrap()),
        "Late cancellation reply changed scope or repeated delivery"
    );
    println!(
        "S1 queued-engine-cancellation: PASS (mutation prevented, one callback, reentrant survivor)"
    );

    // Queue evaluation first, then the gate. Entry proves that JavaScript and result
    // serialization finished, while its callback remains pending in the embedder.
    let calls = cancellation_calls.clone();
    let (sender, completed_reply) = mpsc::channel();
    let completed = cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "window.mutations++;'already-ran'",
            move |result| {
                calls.set(calls.get() + 1);
                let _ = sender.send(result);
            },
        )
        .unwrap();
    let late_cancel = cancel_view.clone();
    cancel_view.with_paused_script_for_document_scope_probe(|| {
        ensure!(
            late_cancel.cancel_javascript_evaluation(completed),
            "Completed reply was not pending"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    let completed_result = completed_reply.try_recv()?;
    ensure!(
        matches!(completed_result, Err(JavaScriptEvaluationError::Cancelled)),
        "Late cancellation returned a success"
    );
    cancel_delegate.reject(cancel_ticket, completed_result)?;
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "1",
    )?;
    ensure!(
        cancellation_calls.get() == 2,
        "Late reply repeated a terminal callback"
    );
    // Cancellation after host delivery is a no-op too.
    let (sender, delivered) = mpsc::channel();
    let delivered_id = cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "'delivered'",
            move |result| {
                let _ = sender.send(result);
            },
        )
        .unwrap();
    text(
        receive(&servo, deadline, delivered)?,
        cancel_document,
        "delivered",
    )?;
    ensure!(
        !cancel_view.cancel_javascript_evaluation(delivered_id),
        "Delivered request was cancelled"
    );
    println!(
        "S1 completed-engine-cancellation: PASS (delivery suppressed, existing mutation preserved)"
    );

    let capture = Rc::new(());
    let released_capture = Rc::downgrade(&capture);
    let calls = cancellation_calls.clone();
    let (sender, panic_cancelled) = mpsc::channel();
    cancel_view.with_paused_script_for_document_scope_probe(|| {
        let id = cancel_view
            .evaluate_cancellable_javascript_in_document(
                cancel_document,
                "window.mutations++;'must-not-run'",
                move |result| {
                    let _capture = capture;
                    calls.set(calls.get() + 1);
                    let _ = sender.send(result);
                    panic!("{CALLBACK_PANIC}");
                },
            )
            .unwrap();
        expect_callback_panic(|| {
            cancel_view.cancel_javascript_evaluation(id);
        })?;
        ensure!(
            !cancel_view.cancel_javascript_evaluation(id),
            "Panicking callback remained pending"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    ensure!(
        released_capture.upgrade().is_none(),
        "Panicking cancellation retained captures"
    );
    ensure!(
        matches!(
            panic_cancelled.try_recv()?,
            Err(JavaScriptEvaluationError::Cancelled)
        ),
        "Panicking cancellation missed terminal delivery"
    );
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "1",
    )?;
    ensure!(
        cancellation_calls.get() == 3
            && cancel_view.pending_evaluations_for_document_scope_probe() == 0,
        "Panicking cancellation repeated delivery or retained work"
    );
    println!(
        "S1 engine-cancellation-callback-panic: PASS (permit closed before unwind, mutation prevented)"
    );

    // Interrupt JavaScript that already passed the execution check. The running script logs
    // entry, and the host cancels only after that message arrives. If interruption fails,
    // the bounded loop completes and the mutation counter records both later increments.
    let calls = cancellation_calls.clone();
    let (sender, interrupted_reply) = mpsc::channel();
    let interrupted = cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "window.mutations++;console.log('s1-interrupt-entered');\
             try{const end=Date.now()+4000;while(Date.now()<end){}}\
             catch(e){window.mutations+=100}finally{window.mutations+=1000}\
             window.mutations++;'must-not-finish'",
            move |result| {
                calls.set(calls.get() + 1);
                let _ = sender.send(result);
            },
        )
        .unwrap();
    wait_console(&servo, &cancel_delegate, deadline, "s1-interrupt-entered")?;
    let interrupt_started = Instant::now();
    ensure!(
        cancel_view.cancel_javascript_evaluation(interrupted),
        "Interruption missed running work"
    );
    let interrupted_result = interrupted_reply.try_recv()?;
    ensure!(
        matches!(
            interrupted_result,
            Err(JavaScriptEvaluationError::Cancelled)
        ),
        "Interruption did not deliver Cancelled: {interrupted_result:?}"
    );
    cancel_delegate.reject(cancel_ticket, interrupted_result)?;
    // The follow-up runs on the same script thread, so it waits for the interrupted script.
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "2",
    )?;
    let interrupt_elapsed = interrupt_started.elapsed();
    ensure!(
        interrupt_elapsed < Duration::from_secs(2),
        "Interruption waited for the loop: {interrupt_elapsed:?}"
    );
    ensure!(
        cancellation_calls.get() == 4
            && cancel_view.pending_evaluations_for_document_scope_probe() == 0
            && cancel_delegate.active.get() == Some(cancel_document)
            && cancel_delegate.generation.get() == cancel_ticket.generation
            && !cancel_delegate.crashed.get(),
        "Interruption repeated delivery, retained work, or changed scope"
    );
    println!(
        "S1 active-engine-interruption: PASS (loop and finally terminated, one callback, {} us to next result)",
        interrupt_elapsed.as_micros()
    );

    // A result getter runs during serialization, after the execution check.
    let (sender, serialization_reply) = mpsc::channel();
    let serialized = cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "window.mutations++;({get value(){console.log('s1-serialization-entered');\
             const end=Date.now()+4000;while(Date.now()<end){}\
             window.mutations++;return 'must-not-serialize'}})",
            move |result| {
                let _ = sender.send(result);
            },
        )
        .unwrap();
    wait_console(
        &servo,
        &cancel_delegate,
        deadline,
        "s1-serialization-entered",
    )?;
    let serialization_started = Instant::now();
    ensure!(
        cancel_view.cancel_javascript_evaluation(serialized),
        "Serialization interruption missed pending work"
    );
    ensure!(
        matches!(
            serialization_reply.try_recv()?,
            Err(JavaScriptEvaluationError::Cancelled)
        ),
        "Serialization interruption did not deliver Cancelled"
    );
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "3",
    )?;
    let serialization_elapsed = serialization_started.elapsed();
    ensure!(
        serialization_elapsed < Duration::from_secs(2),
        "Serialization interruption waited for the getter: {serialization_elapsed:?}"
    );
    println!(
        "S1 serialization-interruption: PASS (getter terminated, {} us to next result)",
        serialization_elapsed.as_micros()
    );

    // Interruption terminates the running script. It does not revoke work that the script
    // already scheduled: the microtask checkpoint follows termination, and timers still fire.
    let (sender, scheduled_reply) = mpsc::channel();
    let scheduled = cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "Promise.resolve().then(()=>{window.s1Microtask=true});\
             setTimeout(()=>{window.s1Timer=true},0);\
             console.log('s1-scheduled-entered');\
             {const end=Date.now()+4000;while(Date.now()<end){}}'must-not-finish'",
            move |result| {
                let _ = sender.send(result);
            },
        )
        .unwrap();
    wait_console(&servo, &cancel_delegate, deadline, "s1-scheduled-entered")?;
    ensure!(
        cancel_view.cancel_javascript_evaluation(scheduled),
        "Scheduled-work interruption missed pending work"
    );
    ensure!(
        matches!(
            scheduled_reply.try_recv()?,
            Err(JavaScriptEvaluationError::Cancelled)
        ),
        "Scheduled-work interruption did not deliver Cancelled"
    );
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.s1Microtask===true)",
        )?,
        cancel_document,
        "true",
    )?;
    loop {
        let timer = scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.s1Timer===true)",
        )?;
        if text(timer, cancel_document, "true").is_ok() {
            break;
        }
        ensure!(Instant::now() < deadline, "Scheduled timer never ran");
    }
    ensure!(
        cancel_delegate.active.get() == Some(cancel_document)
            && cancel_delegate.generation.get() == cancel_ticket.generation,
        "Scheduled work changed scope"
    );
    println!(
        "S1 interrupted-scheduled-work: PASS (prior microtask and timer survived interruption)"
    );

    // The watcher must not interrupt work whose permit remains open.
    let (sender, long_reply) = mpsc::channel();
    let long_started = Instant::now();
    cancel_view
        .evaluate_cancellable_javascript_in_document(
            cancel_document,
            "{const end=Date.now()+300;while(Date.now()<end){}}\
             window.mutations++;'long-complete'",
            move |result| {
                let _ = sender.send(result);
            },
        )
        .unwrap();
    text(
        receive(&servo, deadline, long_reply)?,
        cancel_document,
        "long-complete",
    )?;
    // Date.now() has millisecond precision. The bound spans many watcher intervals.
    let long_elapsed = long_started.elapsed();
    ensure!(
        long_elapsed >= Duration::from_millis(250),
        "Long evaluation returned early: {long_elapsed:?}"
    );
    text(
        scoped(
            &servo,
            &cancel_view,
            deadline,
            cancel_document,
            "String(window.mutations)",
        )?,
        cancel_document,
        "4",
    )?;
    ensure!(
        cancel_view.pending_evaluations_for_document_scope_probe() == 0
            && !cancel_delegate.crashed.get(),
        "Long evaluation retained work or crashed"
    );
    drop(cancel_view);
    println!(
        "S1 uninterrupted-long-evaluation: PASS ({} ms loop completed with an open permit)",
        long_elapsed.as_millis()
    );

    // A non-string JavaScript URL preserves the document. A string result replaces it.
    evaluate(
        &servo,
        &view,
        deadline,
        "location.href=\"javascript:window.marker='js-preserved';void 0\";'scheduled'",
    )?;
    wait_marker(&servo, &view, deadline, "js-preserved")?;
    ensure!(
        identity(&servo, &view, deadline)? == after_delivery,
        "Non-string JavaScript URL replaced the scope"
    );
    let js_ticket = delegate.ticket(after_delivery)?;
    let js_reply = scoped(&servo, &view, deadline, after_delivery, "'before-js-url'")?;
    ensure!(js_reply.is_ok(), "JavaScript URL control failed");
    evaluate(
        &servo,
        &view,
        deadline,
        "location.href=\"javascript:'<h1>JS replacement</h1><script>window.marker=\\\"js-replaced\\\"</script>'\";'scheduled'",
    )?;
    wait_marker(&servo, &view, deadline, "js-replaced")?;
    let after_js = identity(&servo, &view, deadline)?;
    ensure!(
        after_js != after_delivery,
        "JavaScript URL replacement preserved the scope"
    );
    delegate.reject(js_ticket, js_reply)?;
    stale(scoped(
        &servo,
        &view,
        deadline,
        after_delivery,
        "document.querySelector('h1').textContent='WRONG';'mutated'",
    )?)?;
    text(
        scoped(
            &servo,
            &view,
            deadline,
            after_js,
            "document.querySelector('h1').textContent",
        )?,
        after_js,
        "JS replacement",
    )?;
    println!("S1 javascript-url-replacement: PASS");

    let before_reload = after_js;
    view.reload();
    loop {
        ensure!(
            Instant::now() < deadline,
            "Reload did not replace the document before the deadline"
        );
        if let Ok(current) = identity(&servo, &view, deadline)
            && current != before_reload
        {
            break;
        }
        thread::park_timeout(Duration::from_millis(1));
    }
    stale(scoped(
        &servo,
        &view,
        deadline,
        before_reload,
        "'obsolete'",
    )?)?;
    println!("S1 reload: PASS");

    load(&servo, &view, deadline, "/history-a")?;
    let history_a = identity(&servo, &view, deadline)?;
    let history_ticket = delegate.ticket(history_a)?;
    let history_reply = scoped(&servo, &view, deadline, history_a, "'old-activation'")?;
    ensure!(history_reply.is_ok(), "History control did not succeed");
    load(&servo, &view, deadline, "/history-b")?;
    let history_b = identity(&servo, &view, deadline)?;
    let traversal = view.go_back(1);
    drive_until(&servo, deadline, || {
        delegate.traversals.borrow().contains(&traversal)
    })?;
    wait_marker(&servo, &view, deadline, "/history-a")?;
    let restored = identity(&servo, &view, deadline)?;
    ensure!(
        restored != history_b,
        "History selected the departed document"
    );
    stale(scoped(&servo, &view, deadline, history_b, "'obsolete'")?)?;
    text(
        scoped(&servo, &view, deadline, restored, "String(window.marker)")?,
        restored,
        "/history-a",
    )?;
    ensure!(
        restored != history_a,
        "History reactivation revived the old execution scope"
    );
    ensure!(
        history_reply
            .as_ref()
            .is_ok_and(|reply| reply.document == history_a),
        "History control lost its original identity"
    );
    ensure!(
        delegate.generation.get() != history_ticket.generation,
        "History revived the old activation generation"
    );
    stale(scoped(
        &servo,
        &view,
        deadline,
        history_a,
        "document.querySelector('h1').textContent='WRONG';'mutated'",
    )?)?;
    text(
        scoped(
            &servo,
            &view,
            deadline,
            restored,
            "document.querySelector('h1').textContent",
        )?,
        restored,
        "/history-a",
    )?;
    delegate.reject(history_ticket, history_reply)?;
    ensure!(
        delegate.active.get() == Some(restored),
        "History activation missing"
    );
    println!(
        "S1 history: PASS (retained_identity={})",
        restored == history_a
    );

    let before_redirect = restored;
    let redirect_ticket = delegate.ticket(before_redirect)?;
    let redirect_reply = scoped(
        &servo,
        &view,
        deadline,
        before_redirect,
        "'before-redirect'",
    )?;
    ensure!(
        redirect_reply.is_ok(),
        "Expected a retained successful reply"
    );
    let (port, redirect_worker) = redirect_fixture()?;
    delegate.loopback_port.set(port);
    view.load(url::Url::parse(&format!(
        "http://127.0.0.1:{port}/redirect"
    ))?);
    drive_until(&servo, deadline, || redirect_worker.is_finished())?;
    redirect_worker
        .join()
        .map_err(|_| anyhow::anyhow!("Loopback fixture panicked"))??;
    wait_marker(&servo, &view, deadline, "/redirect-final")?;
    delegate.loopback_port.set(0);
    let redirected = identity(&servo, &view, deadline)?;
    ensure!(redirected != before_redirect, "Redirect retained identity");
    delegate.reject(redirect_ticket, redirect_reply)?;
    stale(scoped(
        &servo,
        &view,
        deadline,
        before_redirect,
        "'obsolete'",
    )?)?;
    println!("S1 redirect: PASS");

    let failed_ticket = delegate.ticket(redirected)?;
    let failed_reply = scoped(&servo, &view, deadline, redirected, "'before-failure'")?;
    ensure!(failed_reply.is_ok(), "Expected a retained successful reply");
    view.load(url::Url::parse("s1-failure://fixture/failed")?);
    drive_until(&servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != redirected)
    })?;
    let error_document = identity(&servo, &view, deadline)?;
    delegate.reject(failed_ticket, failed_reply)?;
    stale(scoped(&servo, &view, deadline, redirected, "'obsolete'")?)?;
    println!("S1 failed-load: PASS (error document replaces departed document)");

    let other_delegate = Rc::new(Delegate::default());
    let other = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/other"))?)
        .delegate(other_delegate.clone())
        .build();
    drive_until(&servo, deadline, || other_delegate.loads.get() > 0)?;
    let other_document = identity(&servo, &other, deadline)?;
    stale(scoped(
        &servo,
        &view,
        deadline,
        other_document,
        "'wrong-view'",
    )?)?;
    stale(scoped(
        &servo,
        &other,
        deadline,
        error_document,
        "'wrong-view'",
    )?)?;
    println!("S1 cross-view: PASS");
    let events_before_child_reset = other_delegate.events.borrow().len();
    text(
        scoped(
            &servo,
            &other,
            deadline,
            other_document,
            "const child=document.querySelector('iframe').contentDocument;child.open();child.write('<h1>Child reset</h1>');child.close();'child-reset'",
        )?,
        other_document,
        "child-reset",
    )?;
    ensure!(
        identity(&servo, &other, deadline)? == other_document,
        "Child reset changed top identity"
    );
    ensure!(
        other_delegate.events.borrow()[events_before_child_reset..]
            .iter()
            .all(|event| match event {
                DocumentLifecycle::Active(Some(id))
                | DocumentLifecycle::DomReady(id)
                | DocumentLifecycle::LoadComplete(id) => *id == other_document,
                DocumentLifecycle::Active(None) => false,
            }),
        "Child lifecycle escaped the top-document filter"
    );
    println!("S1 subframe-isolation: PASS");
    let close_ticket = other_delegate.ticket(other_document)?;
    let close_reply = scoped(&servo, &other, deadline, other_document, "'before-close'")?;
    ensure!(close_reply.is_ok(), "Expected a retained successful reply");
    other.close_for_document_scope_probe();
    let close_pending = queue(&other, other_document, "'must-not-deliver'");
    let (sender, close_unscoped) = mpsc::channel();
    other.evaluate_javascript("'must-not-deliver'", move |result| {
        let _ = sender.send(result);
    });
    let survivor = queue(&view, error_document, "'survived-close'");
    drive_until(&servo, deadline, || other_delegate.closed.get())?;
    ensure!(
        other_delegate.active.get().is_none(),
        "Close did not revoke identity"
    );
    other_delegate.reject(close_ticket, close_reply)?;
    ensure!(
        receive(&servo, deadline, close_pending)?.is_err(),
        "Close delivered a pending success"
    );
    ensure!(
        receive(&servo, deadline, close_unscoped)?.is_err(),
        "Close delivered an unscoped pending success"
    );
    text(
        receive(&servo, deadline, survivor)?,
        error_document,
        "survived-close",
    )?;
    ensure!(
        other.pending_evaluations_for_document_scope_probe() == 0,
        "Close retained pending callbacks"
    );
    println!("S1 close: PASS");

    let crash_ticket = delegate.ticket(error_document)?;
    let crash_reply = scoped(&servo, &view, deadline, error_document, "'before-crash'")?;
    ensure!(crash_reply.is_ok(), "Expected a retained successful reply");
    view.crash_for_document_scope_probe();
    drive_until(&servo, deadline, || delegate.crashed.get())?;
    ensure!(
        delegate.active.get() != Some(error_document),
        "Crash did not revoke identity"
    );
    delegate.reject(crash_ticket, crash_reply)?;
    println!("S1 injected-crash: PASS (constellation panic handler)");

    // Use an independent view for a real script-thread unwind. Never show a second view.
    let failure_delegate = Rc::new(Delegate::default());
    let failure = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/thread-failure"))?)
        .delegate(failure_delegate.clone())
        .build();
    drive_until(&servo, deadline, || failure_delegate.loads.get() > 0)?;
    let failure_id = identity(&servo, &failure, deadline)?;
    let failure_ticket = failure_delegate.ticket(failure_id)?;
    let failure_reply = scoped(&servo, &failure, deadline, failure_id, "'before-unwind'")?;
    ensure!(failure_reply.is_ok(), "Thread failure control failed");
    failure.crash_script_thread_for_document_scope_probe();
    let capture = Rc::new(());
    let released = Rc::downgrade(&capture);
    let calls = Rc::new(Cell::new(0));
    let (sender, pending) = mpsc::channel();
    for _ in 0..8 {
        let capture = capture.clone();
        let calls = calls.clone();
        let sender = sender.clone();
        let delegate = failure_delegate.clone();
        failure.evaluate_javascript_in_document(failure_id, "'must-not-execute'", move |result| {
            drop(capture);
            calls.set(calls.get() + 1);
            let _ = sender.send((delegate.active.get(), result));
        });
    }
    drop(capture);
    drop(sender);
    ensure!(
        failure.pending_evaluations_for_document_scope_probe() == 8,
        "Failure fixture did not retain eight callbacks"
    );
    drive_until(&servo, deadline, || failure_delegate.crashed.get())?;
    ensure!(
        failure_delegate
            .crash_reasons
            .borrow()
            .iter()
            .any(|reason| reason == "S1 probe actual script-thread panic"),
        "Thread failure did not reach the unwind notification path"
    );
    ensure!(
        failure_delegate.active.get() != Some(failure_id),
        "Thread unwind preserved the scope"
    );
    for _ in 0..8 {
        let (active_at_callback, result) = pending.recv_timeout(Duration::from_secs(1))?;
        ensure!(
            result.is_err(),
            "Unwound script thread returned a successful result"
        );
        if matches!(result, Err(JavaScriptEvaluationError::StaleDocument)) {
            ensure!(
                active_at_callback != Some(failure_id),
                "Callback preceded host revocation"
            );
        }
    }
    ensure!(
        calls.get() == 8 && pending.try_recv().is_err(),
        "Callback completed more than once"
    );
    ensure!(
        released.upgrade().is_none(),
        "Thread failure retained callback captures"
    );
    ensure!(
        failure.pending_evaluations_for_document_scope_probe() == 0,
        "Thread failure retained pending callbacks"
    );
    failure_delegate.reject(failure_ticket, failure_reply)?;
    // Recovery must accept new work after revocation and drain any late replies safely.
    drive_until(&servo, deadline, || {
        failure_delegate
            .active
            .get()
            .is_some_and(|id| id != failure_id)
    })?;
    let crash_document = identity(&servo, &failure, deadline)?;
    text(
        scoped(
            &servo,
            &failure,
            deadline,
            crash_document,
            "'crash-document'",
        )?,
        crash_document,
        "crash-document",
    )?;
    load(&servo, &failure, deadline, "/after-thread-failure")?;
    let recovered = identity(&servo, &failure, deadline)?;
    ensure!(recovered != failure_id, "Recovery revived the failed scope");
    stale(scoped(
        &servo,
        &failure,
        deadline,
        failure_id,
        "'obsolete'",
    )?)?;
    text(
        scoped(&servo, &failure, deadline, recovered, "'recovered'")?,
        recovered,
        "recovered",
    )?;
    ensure!(calls.get() == 8, "Recovery delivered a duplicate callback");
    println!("S1 script-thread-failure-and-callback-cleanup: PASS (8 callbacks released)");
    // Releasing one clone must preserve the scope. Releasing the last handle must
    // revoke host state and callbacks without waiting for another event-loop drain.
    let release_delegate = Rc::new(Delegate::default());
    let release_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/final-release"))?)
        .delegate(release_delegate.clone())
        .build();
    drive_until(&servo, deadline, || release_delegate.loads.get() > 0)?;
    let release_id = identity(&servo, &release_view, deadline)?;
    let release_ticket = release_delegate.ticket(release_id)?;
    let release_reply = scoped(&servo, &release_view, deadline, release_id, "'retained'")?;
    ensure!(release_reply.is_ok(), "Final-release control failed");
    let release_audit =
        CleanupAudit::queue(&release_view, Some(release_id), &release_delegate, false);
    let clone = release_view.clone();
    drop(clone);
    ensure!(
        !release_delegate.released.get() && release_audit.calls.get() == 0,
        "Non-final handle release revoked the view"
    );
    let (sender, reentrant_release) = mpsc::channel();
    let survivor = failure.clone();
    release_view.evaluate_javascript_in_document(release_id, "'pending'", move |result| {
        if !matches!(result, Err(JavaScriptEvaluationError::StaleDocument)) {
            return;
        }
        survivor.evaluate_javascript_in_document(recovered, "'survived-release'", move |result| {
            let _ = sender.send(result);
        });
    });
    drop(release_view);
    ensure!(
        release_delegate.released.get(),
        "Missing synchronous final-release notification"
    );
    release_audit.check()?;
    release_delegate.reject(release_ticket, release_reply)?;
    text(
        receive(&servo, deadline, reentrant_release)?,
        recovered,
        "survived-release",
    )?;
    ensure!(
        release_audit.calls.get() == 8,
        "Late reply repeated a final-release callback"
    );
    println!(
        "S1 final-handle-release: PASS (mixed callbacks, host revocation, reentrant survivor)"
    );

    // No document identity or activation has reached the host yet.
    let initial_delegate = Rc::new(Delegate::default());
    let initial = WebViewBuilder::new(&servo, context.clone())
        .delegate(initial_delegate.clone())
        .build();
    let initial_audit = CleanupAudit::queue(&initial, None, &initial_delegate, false);
    ensure!(
        initial_delegate.events.borrow().is_empty(),
        "Initial-release fixture already activated"
    );
    drop(initial);
    initial_audit.check()?;
    ensure!(
        initial_delegate.released.get(),
        "Initial release did not notify the host"
    );
    println!("S1 release-before-document-activation: PASS (8 callbacks released)");

    // A normal result callback also runs without an evaluator borrow. Its unwind
    // must leave the pending table usable and its captures released.
    let capture = Rc::new(());
    let released_result = Rc::downgrade(&capture);
    let (sender, panicking_result) = mpsc::channel();
    failure.evaluate_javascript_in_document(recovered, "'before-callback-panic'", move |result| {
        let _capture = capture;
        let _ = sender.send(result);
        panic!("{CALLBACK_PANIC}");
    });
    expect_callback_panic(|| {
        loop {
            assert!(
                Instant::now() < deadline,
                "Callback panic deadline exceeded"
            );
            servo.spin_event_loop();
            thread::park_timeout(Duration::from_millis(1));
        }
    })?;
    text(
        panicking_result.try_recv()?,
        recovered,
        "before-callback-panic",
    )?;
    ensure!(
        released_result.upgrade().is_none(),
        "Result panic retained captures"
    );
    ensure!(
        failure.pending_evaluations_for_document_scope_probe() == 0,
        "Result panic retained a pending callback"
    );
    text(
        scoped(
            &servo,
            &failure,
            deadline,
            recovered,
            "'after-callback-panic'",
        )?,
        recovered,
        "after-callback-panic",
    )?;
    println!(
        "S1 result-callback-panic: PASS (unwind propagated, captures released, evaluation resumed)"
    );

    let panic_delegate = Rc::new(Delegate::default());
    let panic_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/callback-panic-close"))?)
        .delegate(panic_delegate.clone())
        .build();
    drive_until(&servo, deadline, || panic_delegate.loads.get() > 0)?;
    let panic_id = identity(&servo, &panic_view, deadline)?;
    let panic_ticket = panic_delegate.ticket(panic_id)?;
    let panic_reply = scoped(
        &servo,
        &panic_view,
        deadline,
        panic_id,
        "'before-panic-close'",
    )?;
    ensure!(panic_reply.is_ok(), "Panic-close control failed");
    panic_view.close_for_document_scope_probe();
    let panic_close =
        CleanupAudit::queue_with_panics(&panic_view, Some(panic_id), &panic_delegate, false, true);
    expect_callback_panic(|| {
        while panic_close.calls.get() == 0 {
            assert!(
                Instant::now() < deadline,
                "Callback panic deadline exceeded"
            );
            servo.spin_event_loop();
            thread::park_timeout(Duration::from_millis(1));
        }
    })?;
    panic_close.check()?;
    panic_delegate.reject(panic_ticket, panic_reply)?;
    drive_until(&servo, deadline, || panic_delegate.closed.get())?;
    ensure!(
        panic_view.pending_evaluations_for_document_scope_probe() == 0,
        "Lifecycle panic retained callbacks"
    );
    ensure!(
        panic_close.calls.get() == 8,
        "Lifecycle panic duplicated callbacks"
    );
    println!(
        "S1 lifecycle-callback-panics: PASS (8 terminal callbacks, host revoked, unwind propagated)"
    );

    let release_panic_delegate = Rc::new(Delegate::default());
    let release_panic_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!(
            "{ORIGIN}/callback-panic-release"
        ))?)
        .delegate(release_panic_delegate.clone())
        .build();
    drive_until(&servo, deadline, || release_panic_delegate.loads.get() > 0)?;
    let release_panic_id = identity(&servo, &release_panic_view, deadline)?;
    let release_panic = CleanupAudit::queue_with_panics(
        &release_panic_view,
        Some(release_panic_id),
        &release_panic_delegate,
        false,
        true,
    );
    expect_callback_panic(|| drop(release_panic_view))?;
    release_panic.check()?;
    ensure!(
        release_panic_delegate.released.get(),
        "Panic release missed host notification"
    );
    text(
        scoped(
            &servo,
            &failure,
            deadline,
            recovered,
            "'survived-callback-panics'",
        )?,
        recovered,
        "survived-callback-panics",
    )?;
    ensure!(
        release_panic.calls.get() == 8,
        "Late reply repeated panicking callbacks"
    );
    println!(
        "S1 release-callback-panics: PASS (8 terminal callbacks, captures released, survivor usable)"
    );

    // Destruction during a pre-existing host unwind must still finish the batch
    // without replacing the original panic or causing a double-panic abort.
    let unwind_delegate = Rc::new(Delegate::default());
    let unwind_view = WebViewBuilder::new(&servo, context.clone())
        .delegate(unwind_delegate.clone())
        .build();
    let unwind_audit =
        CleanupAudit::queue_with_panics(&unwind_view, None, &unwind_delegate, false, true);
    let outer = catch_unwind(AssertUnwindSafe(|| {
        let _owned_view = unwind_view;
        panic!("S1 probe outer unwind");
    }))
    .expect_err("Outer unwind disappeared");
    ensure!(
        outer.downcast_ref::<&str>() == Some(&"S1 probe outer unwind"),
        "Cleanup replaced the outer unwind"
    );
    unwind_audit.check()?;
    ensure!(
        unwind_delegate.released.get(),
        "Unwind release missed host notification"
    );
    println!(
        "S1 release-during-host-unwind: PASS (8 terminal callbacks, original panic preserved)"
    );

    // The delegate panics after it revokes host state. Its unwind must not drop removed
    // callbacks without delivery, and it takes precedence over later callback panics.
    let lifecycle_panic_delegate = Rc::new(Delegate::default());
    let lifecycle_panic_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/delegate-panic-close"))?)
        .delegate(lifecycle_panic_delegate.clone())
        .build();
    drive_until(&servo, deadline, || {
        lifecycle_panic_delegate.loads.get() > 0
    })?;
    let lifecycle_panic_id = identity(&servo, &lifecycle_panic_view, deadline)?;
    let lifecycle_panic_ticket = lifecycle_panic_delegate.ticket(lifecycle_panic_id)?;
    let lifecycle_panic_reply = scoped(
        &servo,
        &lifecycle_panic_view,
        deadline,
        lifecycle_panic_id,
        "'before-delegate-panic'",
    )?;
    ensure!(
        lifecycle_panic_reply.is_ok(),
        "Delegate-panic control failed"
    );
    lifecycle_panic_delegate.panic_on_revoke.set(true);
    lifecycle_panic_view.close_for_document_scope_probe();
    let lifecycle_panic = CleanupAudit::queue_with_panics(
        &lifecycle_panic_view,
        Some(lifecycle_panic_id),
        &lifecycle_panic_delegate,
        false,
        true,
    );
    expect_panic(DELEGATE_PANIC, || {
        while lifecycle_panic.calls.get() == 0 {
            assert!(
                Instant::now() < deadline,
                "Delegate panic deadline exceeded"
            );
            servo.spin_event_loop();
            thread::park_timeout(Duration::from_millis(1));
        }
    })?;
    ensure!(
        !lifecycle_panic_delegate.panic_on_revoke.get(),
        "Lifecycle revocation did not reach the delegate"
    );
    lifecycle_panic.check()?;
    lifecycle_panic_delegate.reject(lifecycle_panic_ticket, lifecycle_panic_reply)?;
    drive_until(&servo, deadline, || lifecycle_panic_delegate.closed.get())?;
    ensure!(
        lifecycle_panic_view.pending_evaluations_for_document_scope_probe() == 0
            && lifecycle_panic.calls.get() == 8,
        "Delegate panic retained or duplicated callbacks"
    );
    println!(
        "S1 lifecycle-delegate-panic: PASS (8 terminal callbacks, delegate panic preferred, close delivered)"
    );

    let release_delegate_panic = Rc::new(Delegate::default());
    let release_delegate_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!(
            "{ORIGIN}/delegate-panic-release"
        ))?)
        .delegate(release_delegate_panic.clone())
        .build();
    drive_until(&servo, deadline, || release_delegate_panic.loads.get() > 0)?;
    let release_delegate_id = identity(&servo, &release_delegate_view, deadline)?;
    let release_delegate_audit = CleanupAudit::queue(
        &release_delegate_view,
        Some(release_delegate_id),
        &release_delegate_panic,
        false,
    );
    release_delegate_panic.panic_on_revoke.set(true);
    expect_panic(DELEGATE_PANIC, || drop(release_delegate_view))?;
    ensure!(
        release_delegate_panic.released.get() && !release_delegate_panic.panic_on_revoke.get(),
        "Panic release missed host notification"
    );
    release_delegate_audit.check()?;
    text(
        scoped(
            &servo,
            &failure,
            deadline,
            recovered,
            "'survived-delegate-panic'",
        )?,
        recovered,
        "survived-delegate-panic",
    )?;
    ensure!(
        release_delegate_audit.calls.get() == 8,
        "Late reply repeated delegate-panic callbacks"
    );
    println!(
        "S1 release-delegate-panic: PASS (8 terminal callbacks, captures released, survivor usable)"
    );

    // A delegate panic inside a destructor during a host unwind must not abort the
    // process, replace the outer panic, or skip terminal delivery.
    let unwind_delegate_panic = Rc::new(Delegate::default());
    let unwind_delegate_view = WebViewBuilder::new(&servo, context.clone())
        .delegate(unwind_delegate_panic.clone())
        .build();
    let unwind_delegate_audit = CleanupAudit::queue_with_panics(
        &unwind_delegate_view,
        None,
        &unwind_delegate_panic,
        false,
        true,
    );
    unwind_delegate_panic.panic_on_revoke.set(true);
    expect_panic("S1 probe outer unwind", || {
        let _owned_view = unwind_delegate_view;
        panic!("S1 probe outer unwind");
    })?;
    ensure!(
        unwind_delegate_panic.released.get() && !unwind_delegate_panic.panic_on_revoke.get(),
        "Unwind release missed host notification"
    );
    unwind_delegate_audit.check()?;
    println!(
        "S1 delegate-panic-during-host-unwind: PASS (8 terminal callbacks, original panic preserved)"
    );

    let shutdown_ticket = failure_delegate.ticket(recovered)?;
    let shutdown_reply = scoped(&servo, &failure, deadline, recovered, "'before-shutdown'")?;
    ensure!(shutdown_reply.is_ok(), "Shutdown control failed");
    let shutdown_audit = CleanupAudit::queue(&failure, Some(recovered), &failure_delegate, true);
    let initial_shutdown_delegate = Rc::new(Delegate::default());
    let initial_shutdown = WebViewBuilder::new(&servo, context)
        .delegate(initial_shutdown_delegate.clone())
        .build();
    let initial_shutdown_audit =
        CleanupAudit::queue(&initial_shutdown, None, &initial_shutdown_delegate, true);
    let (sender, reentrant_shutdown) = mpsc::channel();
    let stopped_view = failure.clone();
    failure.evaluate_javascript_in_document(recovered, "'pending'", move |_| {
        stopped_view.request_document_identity(move |result| {
            let _ = sender.send(result);
        });
    });
    // Pending closures are now the only owners of this view. Explicit shutdown must
    // break their ownership cycle before Servo's destructor can ever become reachable.
    drop(initial_shutdown);
    ensure!(
        !initial_shutdown_delegate.released.get(),
        "Cycle fixture lost its retained view"
    );
    let shutdown_panics =
        CleanupAudit::queue_with_panics(&failure, Some(recovered), &failure_delegate, false, true);
    // Both delegates panic. The first delegate panic must not skip the other view.
    failure_delegate.panic_on_revoke.set(true);
    initial_shutdown_delegate.panic_on_revoke.set(true);
    expect_panic(DELEGATE_PANIC, || {
        servo.start_shutdown_for_document_scope_probe()
    })?;
    ensure!(
        !failure_delegate.panic_on_revoke.get() && !initial_shutdown_delegate.panic_on_revoke.get(),
        "A delegate panic skipped revocation of another view"
    );
    shutdown_panics.check()?;
    shutdown_audit.check()?;
    initial_shutdown_audit.check()?;
    ensure!(
        matches!(
            reentrant_shutdown.try_recv()?,
            Err(JavaScriptEvaluationError::StaleDocument)
        ),
        "Shutdown callback admitted new engine work"
    );
    failure_delegate.reject(shutdown_ticket, shutdown_reply)?;
    ensure!(
        failure.pending_evaluations_for_document_scope_probe() == 0
            && initial_shutdown_delegate.released.get(),
        "Shutdown retained pending callbacks"
    );
    // Admission after shutdown must reject scoped and legacy requests too.
    stale(scoped(
        &servo,
        &failure,
        deadline,
        recovered,
        "'after-shutdown'",
    )?)?;
    let (sender, unscoped_shutdown) = mpsc::channel();
    failure.evaluate_javascript("'after-shutdown'", move |result| {
        let _ = sender.send(result);
    });
    ensure!(
        matches!(
            unscoped_shutdown.try_recv()?,
            Err(JavaScriptEvaluationError::StaleDocument)
        ),
        "Legacy evaluator admitted work after shutdown"
    );
    let (sender, cancelled_shutdown) = mpsc::channel();
    ensure!(
        failure
            .evaluate_cancellable_javascript_in_document(
                recovered,
                "'after-shutdown'",
                move |result| {
                    let _ = sender.send(result);
                },
            )
            .is_none(),
        "Shutdown returned a cancellable admission id"
    );
    stale(cancelled_shutdown.try_recv()?)?;
    servo.start_shutdown_for_document_scope_probe();
    drive_until(&servo, deadline, || {
        servo.shutdown_complete_for_document_scope_probe()
    })?;
    ensure!(
        shutdown_audit.calls.get() == 8 && initial_shutdown_audit.calls.get() == 8,
        "Shutdown or late engine replies repeated callback delivery"
    );
    ensure!(
        failure_delegate.active.get().is_none() && initial_shutdown_delegate.active.get().is_none(),
        "Queued lifecycle messages reactivated a stopped engine"
    );
    println!(
        "S1 engine-shutdown-cleanup: PASS (16 callbacks, retained views, admission closed, shutdown complete)"
    );
    ensure!(
        shutdown_panics.calls.get() == 8,
        "Shutdown repeated panicking callbacks"
    );
    println!(
        "S1 shutdown-callback-panics: PASS (all terminal callbacks delivered, admission closed, shutdown complete)"
    );
    println!(
        "S1 shutdown-delegate-panics: PASS (both views revoked, delegate panic preferred, shutdown complete)"
    );
    println!("S1 document-scope prototype: PASS (38 cases; experimental lifecycle and host gate)");
    Ok(())
}
