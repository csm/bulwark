//! Isolated S1 prototype regressions. Requires the experimental Servo patch.
use super::{ORIGIN, Waker, drive_until, evaluate};
use anyhow::{Result, ensure};
use servo::{
    ConsoleLogLevel, CreateNewWebViewRequest, DocumentIdentity, DocumentJavaScriptResult,
    DocumentLifecycle, JSValue, JavaScriptEvaluationError, LoadStatus, Opts, RenderingContext,
    Servo, ServoBuilder, ServoDelegate, ServoError, SoftwareRenderingContext, TraversalId,
    WebResourceLoad, WebResourceResponse, WebView, WebViewBuilder, WebViewDelegate,
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
    // Navigation responses withheld by the host. Their pipelines cannot construct a document.
    held: RefCell<Vec<WebResourceLoad>>,
    // Serve withheld paths as a counting script instead of holding them.
    serve_held: Cell<bool>,
    // Markup for one fixture path that depends on run-time data, such as a loopback port.
    frame_markup: RefCell<Option<(String, String)>>,
    // For each intercepted request of the `javascript:` URL source page, whether it carried a
    // `Content-Type` request header.
    javascript_source_requests: RefCell<Vec<bool>>,
}

// Servo's request interceptor buffers a body until the load finishes, so it cannot stream.
// The partial-response fixtures use the real HTTP loader.
const PARTIAL_PATH: &str = "/partial-parse";
// Servo buffers this many body bytes for MIME sniffing before it creates the DOM.
const SNIFF_BYTES: usize = 1445;
const PARTIAL_SCRIPT: &str = "<script>window.marker='/partial-parse'</script>";
const PARTIAL_BLOCKED: &str = "<script src='/held-parser-script'></script><p id='after-script'>";
const PARTIAL_TAIL: &str = "<script>window.marker='late-body'</script>";
const PARTIAL_RELOAD: &str =
    "<!doctype html><h1>reloaded</h1><script>window.marker='/partial-parse'</script>";

// Synchronous construction fixtures. Servo builds `about:blank` and `srcdoc` documents in
// the script thread without a fetch. Parser-inserted scripts from these paths stay withheld.
const SRCDOC_PARENT: &str = "/srcdoc-parent";
const WRITE_PARENT: &str = "/write-parent";
const OPEN_WRITE_SOURCE: &str = "/open-write-source";
const JS_SOURCE: &str = "/javascript-url-source";
const HELD_SERVED: &str = "window.heldServed = (window.heldServed || 0) + 1";
// Inserted scripts do not run in a document with the sandboxed-scripts flag.
const INSERTED_SCRIPT: &str = "(() => { const s = document.createElement('script'); s.text = 'window.inserted = true'; document.documentElement.append(s); return String(window.inserted); })()";

fn fixture_markup(path: &str) -> &'static str {
    match path {
        "/other" => "<iframe src='/child'></iframe>",
        SRCDOC_PARENT => {
            "<iframe srcdoc=\"<script>window.marker='srcdoc-child'</script><script src='/held-srcdoc-script'></script><p id='after-srcdoc'>\"></iframe>"
        }
        WRITE_PARENT => {
            "<script>document.write(\"<script src='/held-write-script'><\\/script><p id='after-write'>\")</script>"
        }
        _ => "",
    }
}

// A second intercepted site for cross-site child frames. Its documents run in an event loop of
// their own, and each reports its path to its parent.
const FRAME_ORIGIN: &str = "https://frame-site.invalid";

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
        if url.path().starts_with("/held") {
            if self.serve_held.get() {
                let mut response = WebResourceResponse::new(url);
                response
                    .headers
                    .insert("content-type", "text/javascript".parse().unwrap());
                let mut served = load.intercept(response);
                served.send_body_data(HELD_SERVED.as_bytes().to_vec());
                served.finish();
            } else {
                self.held.borrow_mut().push(load);
            }
            return;
        }
        if url.host_str() == Some("127.0.0.1") && url.port() == Some(self.loopback_port.get()) {
            // Use the real HTTP loader for the redirect; interception bypasses that path.
            return;
        }
        if url.scheme() == "s1-failure" {
            // The default loader produces UnsupportedScheme without network access.
            return;
        }
        if url.origin().ascii_serialization() == FRAME_ORIGIN {
            // A cross-site child document reports its path to its parent when it runs.
            let mut response = WebResourceResponse::new(url.clone());
            response
                .headers
                .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
            let mut intercepted = load.intercept(response);
            intercepted.send_body_data(
                format!(
                    "<!doctype html><h1>{}</h1><script>window.marker={:?};parent.postMessage(location.pathname, '*')</script>",
                    url.path(),
                    url.path(),
                )
                .into_bytes(),
            );
            intercepted.finish();
            return;
        }
        if url.origin().ascii_serialization() != ORIGIN {
            load.intercept(WebResourceResponse::new(url)).cancel();
            return;
        }
        if url.path() == JS_SOURCE {
            self.javascript_source_requests
                .borrow_mut()
                .push(load.request().headers.contains_key("content-type"));
        }
        let mut response = WebResourceResponse::new(url.clone());
        response
            .headers
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        let markup = match self.frame_markup.borrow().as_ref() {
            Some((path, markup)) if path == url.path() => markup.clone(),
            _ => fixture_markup(url.path()).to_owned(),
        };
        let mut intercepted = load.intercept(response);
        intercepted.send_body_data(
            format!(
                "<!doctype html><h1>{}</h1><script>window.marker={:?};window.mutations=0</script>{markup}",
                url.path(),
                url.path(),
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

fn open_popup(
    servo: &Servo,
    view: &WebView,
    delegate: &Delegate,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    document: DocumentIdentity,
) -> Result<(WebView, Rc<Delegate>, DocumentIdentity)> {
    let opened = queue(view, document, "window.open('about:blank');'opened'");
    drive_until(servo, deadline, || !delegate.popups.borrow().is_empty())?;
    let popup_delegate = Rc::new(Delegate::default());
    let popup = delegate
        .popups
        .borrow_mut()
        .pop()
        .unwrap()
        .builder(context)
        .delegate(popup_delegate.clone())
        .build();
    text(receive(servo, deadline, opened)?, document, "opened")?;
    let popup_document = identity(servo, &popup, deadline)?;
    ensure!(
        popup_document != document,
        "Popup reused its opener's scope"
    );
    Ok((popup, popup_delegate, popup_document))
}

// Cancel withheld navigation responses. Their pipelines may already be gone.
fn cancel_held(delegate: &Delegate) -> usize {
    let held = delegate.held.take();
    let count = held.len();
    for load in held {
        let url = load.request().url.clone();
        load.intercept(WebResourceResponse::new(url)).cancel();
    }
    count
}

// Servo's crash page embeds the failure reason. A request hook must not replace it.
// The reason text only orders the fixture; it grants no host authority.
fn crash_page(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    document: DocumentIdentity,
    reason: &str,
) -> Result<()> {
    let script = format!(
        "document.documentElement.textContent.includes({reason:?}) ? 'crash-document' : String(location.href)"
    );
    text(
        scoped(servo, view, deadline, document, &script)?,
        document,
        "crash-document",
    )
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
    wait_any_marker(servo, view, deadline, &[path]).map(|_| ())
}

// Wait for any of several markers, so a check after the wait can report the wrong page.
fn wait_any_marker(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    markers: &[&str],
) -> Result<String> {
    // A fixture marker establishes test ordering only; it grants no host authority.
    loop {
        let (sender, receiver) = mpsc::channel();
        view.evaluate_javascript("String(window.marker)", move |result| {
            let _ = sender.send(result);
        });
        match receive(servo, deadline, receiver)? {
            Ok(JSValue::String(marker)) if markers.contains(&marker.as_str()) => {
                return Ok(marker);
            }
            Ok(JSValue::String(_))
            | Err(JavaScriptEvaluationError::StaleDocument)
            | Err(JavaScriptEvaluationError::WebViewNotReady)
            | Err(JavaScriptEvaluationError::DocumentNotFound) => {}
            other => anyhow::bail!("Unexpected navigation marker result: {other:?}"),
        }
        thread::park_timeout(Duration::from_millis(1));
    }
}

fn accept_loopback(listener: &TcpListener, deadline: Instant) -> Result<std::net::TcpStream> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Accepted sockets can inherit the listener's nonblocking mode on macOS.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    Instant::now() < deadline,
                    "Loopback fixture deadline exceeded"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_loopback_request(stream: &mut std::net::TcpStream) -> Result<String> {
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        request.push(byte[0]);
        ensure!(request.len() <= 16384, "Oversized loopback request");
    }
    Ok(String::from_utf8_lossy(&request).into_owned())
}

// Serve the parsing document twice. The first response sends its headers and a partial body,
// then waits for the release signal before it sends the rest. The second response is complete.
// The worker reports whether the socket accepted the held tail, not whether Servo read it.
fn partial_fixture(
    head: String,
) -> Result<(u16, mpsc::Sender<()>, thread::JoinHandle<Result<bool>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let (release, released) = mpsc::channel();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        let expected = format!("GET {PARTIAL_PATH} HTTP/1.1\r\n");
        let mut stream = accept_loopback(&listener, deadline)?;
        ensure!(
            read_loopback_request(&mut stream)?.starts_with(&expected),
            "Unexpected partial request"
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{head}",
            head.len() + PARTIAL_TAIL.len()
        )?;
        stream.flush()?;
        released.recv_timeout(Duration::from_secs(30))?;
        let tail_delivered = stream
            .write_all(PARTIAL_TAIL.as_bytes())
            .and_then(|()| stream.flush())
            .is_ok();
        drop(stream);
        let mut stream = accept_loopback(&listener, deadline)?;
        ensure!(
            read_loopback_request(&mut stream)?.starts_with(&expected),
            "Unexpected reload request"
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PARTIAL_RELOAD}",
            PARTIAL_RELOAD.len()
        )?;
        Ok(tail_delivered)
    });
    Ok((port, release, worker))
}

#[derive(Clone, Copy)]
enum PartialFailure {
    BeforeParse,
    ParserBlocked,
}

impl PartialFailure {
    fn name(self) -> &'static str {
        match self {
            Self::BeforeParse => "headers-before-parse-failure",
            Self::ParserBlocked => "parser-blocked-failure",
        }
    }

    fn head(self) -> String {
        let head = format!("<!doctype html><h1>partial</h1>{PARTIAL_SCRIPT}");
        match self {
            Self::BeforeParse => {
                assert!(head.len() < SNIFF_BYTES);
                head
            }
            // Pad with a comment so the parser starts before the withheld script.
            Self::ParserBlocked => format!(
                "<!doctype html><!--{}--><h1>partial</h1>{PARTIAL_SCRIPT}{PARTIAL_BLOCKED}",
                "p".repeat(SNIFF_BYTES)
            ),
        }
    }
}

fn partial_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    variant: PartialFailure,
) -> Result<String> {
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(servo, context)
        .url(url::Url::parse(&format!("{ORIGIN}/partial-source"))?)
        .delegate(delegate.clone())
        .build();
    drive_until(servo, deadline, || delegate.loads.get() > 0)?;
    let source = identity(servo, &view, deadline)?;
    let (port, release, worker) = partial_fixture(variant.head())?;
    delegate.loopback_port.set(port);
    let origin = format!("http://127.0.0.1:{port}");
    view.load(url::Url::parse(&format!("{origin}{PARTIAL_PATH}"))?);
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != source)
    })?;
    let expected_state = match variant {
        PartialFailure::BeforeParse => "null:undefined",
        PartialFailure::ParserBlocked => {
            wait_marker(servo, &view, deadline, PARTIAL_PATH)?;
            // Servo's prefetch tokenizer requests the script once, and the blocked parser
            // requests it again. Both requests stay withheld.
            drive_until(servo, deadline, || {
                delegate
                    .held
                    .borrow()
                    .iter()
                    .filter(|load| load.request().url.path() == "/held-parser-script")
                    .count()
                    == 2
            })?;
            "loading:true"
        }
    };
    let failed = identity(servo, &view, deadline)?;
    ensure!(
        failed != source && delegate.active.get() == Some(failed),
        "Partial response did not activate a new document"
    );
    // Before parsing there is no DOM and no script has run. A blocked parser has run the
    // first script but has not reached the element after the withheld script.
    let state = scoped(
        servo,
        &view,
        deadline,
        failed,
        match variant {
            PartialFailure::BeforeParse => {
                "String(document.documentElement) + ':' + String(window.marker)"
            }
            PartialFailure::ParserBlocked => {
                "document.readyState + ':' + (document.getElementById('after-script') === null)"
            }
        },
    )?;
    text(state, failed, expected_state)?;
    let ticket = delegate.ticket(failed)?;
    let reply = scoped(servo, &view, deadline, failed, "'before-partial-failure'")?;
    ensure!(reply.is_ok(), "Partial response control failed");
    // The constellation forwards both messages in order, so the unwind precedes these requests.
    view.crash_script_thread_for_document_scope_probe();
    let audit = CleanupAudit::queue(&view, Some(failed), &delegate, false);
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check()?;
    drive_until(servo, deadline, || delegate.crashed.get())?;
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0,
        "Partial response failure retained callbacks"
    );
    delegate.reject(ticket, reply)?;
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != failed)
    })?;
    let crash = identity(servo, &view, deadline)?;
    crash_page(
        servo,
        &view,
        deadline,
        crash,
        "S1 probe actual script-thread panic",
    )?;
    // Late response data for the failed document must not reach any live document.
    let errors_before_release = engine_errors.0.borrow().len();
    let held = delegate.held.take();
    let expected_held = match variant {
        PartialFailure::BeforeParse => 0,
        PartialFailure::ParserBlocked => 2,
    };
    ensure!(
        held.len() == expected_held,
        "Unexpected withheld subresources: {:?}",
        held.iter()
            .map(|load| (load.request().url.to_string(), load.request().destination))
            .collect::<Vec<_>>()
    );
    for load in held {
        let url = load.request().url.clone();
        let mut late = load.intercept(WebResourceResponse::new(url));
        late.send_body_data(b"window.marker='late-script'".to_vec());
        late.finish();
    }
    release.send(())?;
    let settle = Instant::now() + Duration::from_millis(300);
    drive_until(servo, deadline, || Instant::now() >= settle)?;
    ensure!(
        identity(servo, &view, deadline)? == crash,
        "Late response data replaced the crash document"
    );
    crash_page(
        servo,
        &view,
        deadline,
        crash,
        "S1 probe actual script-thread panic",
    )?;
    ensure!(
        !delegate.events.borrow().iter().any(|event| matches!(
            event,
            DocumentLifecycle::DomReady(id) | DocumentLifecycle::LoadComplete(id) if *id == failed
        )),
        "The failed partial document reported readiness"
    );
    ensure!(
        delegate.crash_reasons.borrow().len() == 1,
        "Late response data caused another failure"
    );
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "Late response data raised engine errors: {late_errors:?}"
    );
    stale(scoped(servo, &view, deadline, failed, "'obsolete'")?)?;
    // Reload restores the failed load. The fixture's second response is complete.
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != crash)
    })?;
    wait_marker(servo, &view, deadline, PARTIAL_PATH)?;
    let reloaded = identity(servo, &view, deadline)?;
    text(
        scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
        reloaded,
        &origin,
    )?;
    let tail_delivered = worker
        .join()
        .map_err(|_| anyhow::anyhow!("Partial fixture panicked"))??;
    delegate.loopback_port.set(0);
    load(servo, &view, deadline, "/after-partial-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(servo, &view, deadline, recovered, "'partial-recovered'")?,
        recovered,
        "partial-recovered",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "Partial failure recovery repeated callbacks"
    );
    Ok(format!(
        "active document unwound, 8 callbacks released, late data ignored, tail_delivered={tail_delivered}, reload and new URL recover"
    ))
}

// Poll a scoped evaluation until it returns the expected text. The script must not throw.
fn wait_text(
    servo: &Servo,
    view: &WebView,
    deadline: Instant,
    document: DocumentIdentity,
    script: &str,
    expected: &str,
) -> Result<()> {
    loop {
        let result = scoped(servo, view, deadline, document, script)?;
        if matches!(&result, Ok(reply) if reply.value == JSValue::String(expected.into())) {
            return text(result, document, expected);
        }
        ensure!(result.is_ok(), "Polled evaluation failed: {result:?}");
        thread::park_timeout(Duration::from_millis(1));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Construction {
    // The view's initial `about:blank` document.
    AboutBlank,
    // `document.write` into that document opens a script-created parser.
    AboutBlankWrite,
    // A `srcdoc` child shares the parent's script thread and waits for a withheld script.
    Srcdoc,
    // A parser-inserted script writes a withheld script during the initial parse.
    DocumentWrite,
    // `document.open` resets a complete document, and the written script is withheld.
    OpenWrite,
}

impl Construction {
    fn name(self) -> &'static str {
        match self {
            Self::AboutBlank => "about-blank-failure",
            Self::AboutBlankWrite => "about-blank-write-failure",
            Self::Srcdoc => "srcdoc-parser-blocked-failure",
            Self::DocumentWrite => "document-write-parser-blocked-failure",
            Self::OpenWrite => "document-open-write-failure",
        }
    }

    fn blank(self) -> bool {
        matches!(self, Self::AboutBlank | Self::AboutBlankWrite)
    }

    fn source(self) -> String {
        match self {
            Self::AboutBlank | Self::AboutBlankWrite => "about:blank".into(),
            Self::Srcdoc => format!("{ORIGIN}{SRCDOC_PARENT}"),
            Self::DocumentWrite => format!("{ORIGIN}{WRITE_PARENT}"),
            Self::OpenWrite => format!("{ORIGIN}{OPEN_WRITE_SOURCE}"),
        }
    }

    // A script that resets the complete source document and leaves its parser blocked.
    fn reset(self) -> Option<String> {
        match self {
            // The opaque `about:blank` document has no base for a relative script URL.
            Self::AboutBlankWrite => Some(format!(
                "document.write(\"<h1>written</h1><script src='{ORIGIN}/held-blank-script'><\\/script><p id='after-script'>\");'written'"
            )),
            Self::OpenWrite => Some(
                "document.open();document.write(\"<h1>written</h1><script src='/held-open-script'><\\/script><p id='after-script'>\");'written'".into(),
            ),
            _ => None,
        }
    }

    // Lifecycle readiness events for the failed document before the unwind.
    fn readiness(self) -> &'static [&'static str] {
        match self {
            Self::AboutBlank => &["dom-ready", "load-complete"],
            // The parent finishes parsing, but its load event waits for the blocked child.
            Self::Srcdoc => &["dom-ready"],
            Self::AboutBlankWrite | Self::DocumentWrite | Self::OpenWrite => &[],
        }
    }

    fn held_path(self) -> Option<&'static str> {
        match self {
            Self::AboutBlank => None,
            Self::AboutBlankWrite => Some("/held-blank-script"),
            Self::Srcdoc => Some("/held-srcdoc-script"),
            Self::DocumentWrite => Some("/held-write-script"),
            Self::OpenWrite => Some("/held-open-script"),
        }
    }

    fn state(self) -> (&'static str, &'static str) {
        match self {
            Self::AboutBlank => (
                "String(location.href) + ':' + document.readyState",
                "about:blank:complete",
            ),
            Self::AboutBlankWrite | Self::OpenWrite => (
                "document.readyState + ':' + (document.getElementById('after-script') === null) + ':' + document.querySelector('h1').textContent",
                "loading:true:written",
            ),
            // Synchronous access to the child proves that it shares the parent's script thread.
            Self::Srcdoc => (
                "(() => { const child = frames[0]; return String(child.location.href) + ':' + child.document.readyState + ':' + (child.document.getElementById('after-srcdoc') === null) + ':' + child.marker; })()",
                "about:srcdoc:loading:true:srcdoc-child",
            ),
            Self::DocumentWrite => (
                "document.readyState + ':' + (document.getElementById('after-write') === null)",
                "loading:true",
            ),
        }
    }

    // The reloaded document must be the original load, not the reset or failed state.
    fn reloaded(self) -> (&'static str, &'static str) {
        match self {
            Self::AboutBlank | Self::AboutBlankWrite => (
                "String(location.href) + ':' + document.querySelector('h1') + ':' + String(window.heldServed)",
                "about:blank:null:undefined",
            ),
            Self::Srcdoc => (
                "(() => { try { const child = frames[0]; return String(child.document.getElementById('after-srcdoc') !== null) + ':' + child.heldServed; } catch (error) { return 'pending'; } })()",
                "true:1",
            ),
            Self::DocumentWrite => (
                "String(document.getElementById('after-write') !== null) + ':' + window.heldServed",
                "true:1",
            ),
            Self::OpenWrite => (
                "String(document.querySelector('h1') && document.querySelector('h1').textContent) + ':' + (document.getElementById('after-script') === null) + ':' + String(window.heldServed)",
                "/open-write-source:true:undefined",
            ),
        }
    }
}

fn construction_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    survivor: (&WebView, DocumentIdentity),
    variant: Construction,
) -> Result<String> {
    let reason = "S1 probe actual script-thread panic";
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(servo, context)
        .url(url::Url::parse(&variant.source())?)
        .delegate(delegate.clone())
        .build();
    match variant {
        Construction::AboutBlank | Construction::AboutBlankWrite | Construction::OpenWrite => {
            drive_until(servo, deadline, || delegate.loads.get() > 0)?
        }
        // The load event waits for the blocked script, so wait for activation and the marker.
        // Before activation, the view has no top browsing context.
        Construction::Srcdoc | Construction::DocumentWrite => {
            drive_until(servo, deadline, || delegate.active.get().is_some())?;
            let marker = if variant == Construction::Srcdoc {
                SRCDOC_PARENT
            } else {
                WRITE_PARENT
            };
            wait_marker(servo, &view, deadline, marker)?
        }
    }
    let source = identity(servo, &view, deadline)?;
    if let Some(reset) = variant.reset() {
        // The reset allocates a fresh incarnation, so the engine rejects this result.
        stale(scoped(servo, &view, deadline, source, &reset)?)?;
    }
    let held_count = |delegate: &Delegate| {
        delegate
            .held
            .borrow()
            .iter()
            .filter(|load| Some(load.request().url.path()) == variant.held_path())
            .count()
    };
    if variant.held_path().is_some() {
        drive_until(servo, deadline, || held_count(&delegate) > 0)?;
    }
    let failed = identity(servo, &view, deadline)?;
    ensure!(
        delegate.active.get() == Some(failed),
        "Construction fixture did not activate its document"
    );
    ensure!(
        (failed != source) == variant.reset().is_some(),
        "Unexpected construction reset: source={source:?}, failed={failed:?}"
    );
    if variant.reset().is_some() {
        ensure!(
            failed.pipeline_id() == source.pipeline_id(),
            "Construction reset changed the pipeline"
        );
    }
    let (state, expected_state) = variant.state();
    text(
        scoped(servo, &view, deadline, failed, state)?,
        failed,
        expected_state,
    )?;
    let readiness = || -> Vec<&'static str> {
        delegate
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                DocumentLifecycle::DomReady(id) if *id == failed => Some("dom-ready"),
                DocumentLifecycle::LoadComplete(id) if *id == failed => Some("load-complete"),
                _ => None,
            })
            .collect()
    };
    // The parent's readiness event can follow the child's construction.
    drive_until(servo, deadline, || {
        readiness().len() >= variant.readiness().len()
    })?;
    ensure!(
        readiness() == variant.readiness(),
        "Unexpected readiness before the failure: {:?}",
        readiness()
    );
    let ticket = delegate.ticket(failed)?;
    let reply = scoped(
        servo,
        &view,
        deadline,
        failed,
        "'before-construction-failure'",
    )?;
    ensure!(reply.is_ok(), "Construction failure control failed");
    // The constellation forwards both messages in order, so the unwind precedes these requests.
    view.crash_script_thread_for_document_scope_probe();
    let audit = CleanupAudit::queue(&view, Some(failed), &delegate, false);
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check()?;
    drive_until(servo, deadline, || delegate.crashed.get())?;
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0,
        "Construction failure retained callbacks"
    );
    delegate.reject(ticket, reply)?;
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != failed)
    })?;
    let crash = identity(servo, &view, deadline)?;
    let check_crash = || -> Result<()> {
        // Servo applies the crash page's sandboxing flags only on its error-page path.
        // It loads `about:blank` without the crash fetch, as an ordinary blank document
        // with an opaque origin that runs inserted scripts. The error page runs none.
        let (scripting, expected) = if variant.blank() {
            (
                format!(
                    "String(location.href) + '|' + String(window.origin) + '|' + document.documentElement.textContent.includes({reason:?}) + '|' + {INSERTED_SCRIPT}"
                ),
                "about:blank|null|false|true",
            )
        } else {
            crash_page(servo, &view, deadline, crash, reason)?;
            (
                format!("String(window.origin) + ':' + {INSERTED_SCRIPT}"),
                "null:undefined",
            )
        };
        text(
            scoped(servo, &view, deadline, crash, &scripting)?,
            crash,
            expected,
        )
    };
    check_crash()?;
    let errors_before_release = engine_errors.0.borrow().len();
    let held = delegate.held.take();
    ensure!(
        held.len() == usize::from(variant.held_path().is_some())
            && held
                .iter()
                .all(|load| Some(load.request().url.path()) == variant.held_path()),
        "Unexpected withheld requests: {:?}",
        held.iter()
            .map(|load| load.request().url.to_string())
            .collect::<Vec<_>>()
    );
    // Late script data for the failed document must not reach any live document.
    for load in held {
        let url = load.request().url.clone();
        let mut late = load.intercept(WebResourceResponse::new(url));
        late.send_body_data(b"window.marker='late-script'".to_vec());
        late.finish();
    }
    let settle = Instant::now() + Duration::from_millis(300);
    drive_until(servo, deadline, || Instant::now() >= settle)?;
    ensure!(
        identity(servo, &view, deadline)? == crash,
        "Late script data replaced the crash document"
    );
    check_crash()?;
    ensure!(
        readiness() == variant.readiness(),
        "The failed document reported readiness after the failure"
    );
    ensure!(
        delegate.crash_reasons.borrow().as_slice() == [reason],
        "Unexpected failure notifications: {:?}",
        delegate.crash_reasons.borrow()
    );
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "Late script data raised engine errors: {late_errors:?}"
    );
    stale(scoped(servo, &view, deadline, failed, "'obsolete'")?)?;
    stale(scoped(servo, &view, deadline, source, "'obsolete'")?)?;
    // Reload restores the original load. Withheld paths are now served.
    delegate.serve_held.set(true);
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != crash)
    })?;
    let reloaded = identity(servo, &view, deadline)?;
    ensure!(
        reloaded != failed && reloaded != source,
        "Reload revived an obsolete scope"
    );
    if variant.blank() {
        text(
            scoped(
                servo,
                &view,
                deadline,
                reloaded,
                &format!("String(window.origin) + '|' + {INSERTED_SCRIPT}"),
            )?,
            reloaded,
            "null|true",
        )?;
    } else {
        text(
            scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
            reloaded,
            ORIGIN,
        )?;
    }
    // A fetched reload activates at its response headers, before the parser runs.
    // Wait for the original page's marker before the state check.
    match variant {
        Construction::Srcdoc => wait_marker(servo, &view, deadline, SRCDOC_PARENT)?,
        Construction::DocumentWrite => wait_marker(servo, &view, deadline, WRITE_PARENT)?,
        Construction::OpenWrite => wait_marker(servo, &view, deadline, OPEN_WRITE_SOURCE)?,
        Construction::AboutBlank | Construction::AboutBlankWrite => {}
    }
    let (reloaded_state, expected_reloaded) = variant.reloaded();
    wait_text(
        servo,
        &view,
        deadline,
        reloaded,
        reloaded_state,
        expected_reloaded,
    )?;
    load(servo, &view, deadline, "/after-construction-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(
            servo,
            &view,
            deadline,
            recovered,
            "'construction-recovered'",
        )?,
        recovered,
        "construction-recovered",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'construction-survivor'",
        )?,
        survivor_document,
        "construction-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "Construction failure recovery repeated callbacks"
    );
    Ok(format!(
        "{}, 8 callbacks released, {}, reload and new URL recover, survivor usable",
        match variant {
            Construction::AboutBlank => "initial about:blank unwound",
            Construction::AboutBlankWrite => "about:blank parser blocked by document.write",
            Construction::Srcdoc => "srcdoc child parser blocked in the parent's thread",
            Construction::DocumentWrite => "parser blocked by a written script",
            Construction::OpenWrite => "document.open parser blocked by a written script",
        },
        match variant {
            Construction::AboutBlank => "blank crash document with origin null runs scripts",
            Construction::AboutBlankWrite =>
                "late script ignored, blank crash document with origin null runs scripts",
            _ => "late script ignored, sandboxed error page",
        }
    ))
}

// A `javascript:` URL result replaces the document in a new pipeline. Its load data keeps
// the result and the source document's URL. The experimental patch drops the result from the
// crash load, so crash recovery fetches the error page, and reload fetches the source URL.
fn javascript_url_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    survivor: (&WebView, DocumentIdentity),
) -> Result<String> {
    let reason = "S1 probe actual script-thread panic";
    let result_marker = "js-crash-source";
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(servo, context)
        .url(url::Url::parse(&format!("{ORIGIN}{JS_SOURCE}"))?)
        .delegate(delegate.clone())
        .build();
    drive_until(servo, deadline, || delegate.loads.get() > 0)?;
    let source = identity(servo, &view, deadline)?;
    evaluate(
        servo,
        &view,
        deadline,
        "location.href=\"javascript:'<h1>JS crash source</h1><script>window.marker=\\\"js-crash-source\\\"</script>'\";'scheduled'",
    )?;
    wait_marker(servo, &view, deadline, result_marker)?;
    let failed = identity(servo, &view, deadline)?;
    ensure!(
        failed != source
            && failed.pipeline_id() != source.pipeline_id()
            && delegate.active.get() == Some(failed),
        "The JavaScript URL result did not replace the document in a new pipeline"
    );
    let ticket = delegate.ticket(failed)?;
    let reply = scoped(
        servo,
        &view,
        deadline,
        failed,
        "'before-javascript-url-failure'",
    )?;
    ensure!(reply.is_ok(), "JavaScript URL failure control failed");
    view.crash_script_thread_for_document_scope_probe();
    let audit = CleanupAudit::queue(&view, Some(failed), &delegate, false);
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check()?;
    drive_until(servo, deadline, || delegate.crashed.get())?;
    ensure!(
        delegate.crash_reasons.borrow().as_slice() == [reason],
        "Unexpected failure notifications: {:?}",
        delegate.crash_reasons.borrow()
    );
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0,
        "JavaScript URL failure retained callbacks"
    );
    delegate.reject(ticket, reply)?;
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != failed)
    })?;
    let crash = identity(servo, &view, deadline)?;
    // The crash document is Servo's error page with the failure reason. It keeps the
    // document's URL, has an opaque origin, runs no scripts, and omits the JavaScript result.
    crash_page(servo, &view, deadline, crash, reason)?;
    let result_text = "JS crash source";
    text(
        scoped(
            servo,
            &view,
            deadline,
            crash,
            &format!(
                "String(location.href) + '|' + String(window.origin) + '|' + String(window.marker) + '|' + document.documentElement.textContent.includes({result_text:?}) + '|' + {INSERTED_SCRIPT}"
            ),
        )?,
        crash,
        &format!("{ORIGIN}{JS_SOURCE}|null|undefined|false|undefined"),
    )?;
    // The crash fetch bypasses interception. Only the initial load requested the source page.
    ensure!(
        delegate.javascript_source_requests.borrow().as_slice() == [false],
        "Unexpected source page requests before reload: {:?}",
        delegate.javascript_source_requests.borrow()
    );
    stale(scoped(servo, &view, deadline, failed, "'obsolete'")?)?;
    stale(scoped(servo, &view, deadline, source, "'obsolete'")?)?;
    // Reload fetches the document's URL, which is the source page's URL, as a script-thread
    // reload of the result would. A fetched reload activates at its response headers, so wait
    // for either page's marker before the check.
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != crash)
    })?;
    wait_any_marker(servo, &view, deadline, &[JS_SOURCE, result_marker])?;
    let reloaded = identity(servo, &view, deadline)?;
    ensure!(
        reloaded != crash && reloaded != failed && reloaded != source,
        "Reload revived an obsolete scope"
    );
    text(
        scoped(
            servo,
            &view,
            deadline,
            reloaded,
            &format!(
                "String(location.href) + '|' + String(window.origin) + '|' + document.querySelector('h1').textContent + '|' + String(window.marker) + '|' + document.documentElement.textContent.includes({result_text:?}) + '|' + document.documentElement.textContent.includes({reason:?}) + '|' + {INSERTED_SCRIPT}"
            ),
        )?,
        reloaded,
        &format!("{ORIGIN}{JS_SOURCE}|{ORIGIN}|{JS_SOURCE}|{JS_SOURCE}|false|false|true"),
    )?;
    // The reload request omits the `Content-Type` header that described the JavaScript result.
    ensure!(
        delegate.javascript_source_requests.borrow().as_slice() == [false, false],
        "Unexpected source page requests after reload: {:?}",
        delegate.javascript_source_requests.borrow()
    );
    stale(scoped(servo, &view, deadline, crash, "'obsolete'")?)?;
    load(servo, &view, deadline, "/after-javascript-url-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(
            servo,
            &view,
            deadline,
            recovered,
            "'javascript-url-recovered'",
        )?,
        recovered,
        "javascript-url-recovered",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'javascript-url-survivor'",
        )?,
        survivor_document,
        "javascript-url-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "JavaScript URL failure recovery repeated callbacks"
    );
    Ok("8 callbacks released, error page with origin null runs no scripts and omits the JavaScript result, reload fetches the source page with its origin and scripts and no result header, new URL recovers, survivor usable".into())
}

// Cross-site parsing fixtures. A loopback HTTP server serves each cross-site document, so the
// real loader streams its body. Parser-inserted scripts from `/held` paths stay withheld.
const CROSS_SITE_PARSE: &str = "/cross-site-parse";
const CROSS_SITE_RELOAD: &str =
    "<!doctype html><h1>reloaded</h1><script>window.marker='/cross-site-parse'</script>";
const PARSE_FRAME: &str = "/partial-frame";
const FRAME_SIBLING: &str = "/frame-sibling";
// Child frames report progress to the parent. These messages only order the fixture.
const FRAME_PARSING: &str = "<script>parent.postMessage('frame-parsing', '*')</script>";
const FRAME_RELOAD: &str =
    "<!doctype html><script>parent.postMessage('frame-complete', '*')</script>";
const FRAME_LISTENER: &str = "<script>addEventListener('message', event => { window.frameState = String(event.data) })</script>";

// Pad the body past Servo's MIME-sniffing buffer, so the parser runs the first script and then
// waits for the withheld script.
fn blocked_head(script: &str, held: &str) -> String {
    format!(
        "<!doctype html><!--{}-->{script}<script src='{held}'></script><p id='after-script'>",
        "p".repeat(SNIFF_BYTES)
    )
}

enum ParseCommand {
    Release,
    Stop,
}

struct ParseServed {
    // Whether the socket accepted the held tail. This does not show that Servo read it.
    tail_delivered: Option<bool>,
    paths: Vec<String>,
}

struct ParseServer {
    port: u16,
    commands: mpsc::Sender<ParseCommand>,
    worker: thread::JoinHandle<Result<ParseServed>>,
}

impl ParseServer {
    // The first request for `held` receives its headers and `head`. That response stays open
    // until release. A later request for `held` receives `reload`. Other paths receive a
    // complete page with a marker.
    fn start(held: &'static str, head: String, reload: &'static str) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let (commands, received) = mpsc::channel();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(60);
            let mut held_stream: Option<std::net::TcpStream> = None;
            let mut held_sent = false;
            let mut served = ParseServed {
                tail_delivered: None,
                paths: Vec::new(),
            };
            loop {
                match received.try_recv() {
                    Ok(ParseCommand::Release) => {
                        if let Some(mut stream) = held_stream.take() {
                            served.tail_delivered = Some(
                                stream
                                    .write_all(PARTIAL_TAIL.as_bytes())
                                    .and_then(|()| stream.flush())
                                    .is_ok(),
                            );
                        }
                    }
                    Ok(ParseCommand::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
                        return Ok(served);
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        ensure!(
                            Instant::now() < deadline,
                            "Loopback fixture deadline exceeded"
                        );
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                };
                // Accepted sockets can inherit the listener's nonblocking mode on macOS.
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                let request = read_loopback_request(&mut stream)?;
                let path = request.split(' ').nth(1).unwrap_or_default().to_owned();
                served.paths.push(path.clone());
                if path == held && !held_sent {
                    held_sent = true;
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{head}",
                        head.len() + PARTIAL_TAIL.len()
                    )?;
                    stream.flush()?;
                    held_stream = Some(stream);
                    continue;
                }
                let body = if path == held {
                    reload.to_owned()
                } else {
                    format!("<!doctype html><h1>{path}</h1><script>window.marker={path:?}</script>")
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
            }
        });
        Ok(Self {
            port,
            commands,
            worker,
        })
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn release(&self) -> Result<()> {
        self.commands
            .send(ParseCommand::Release)
            .map_err(|_| anyhow::anyhow!("Loopback fixture stopped early"))
    }

    fn finish(self) -> Result<ParseServed> {
        let _ = self.commands.send(ParseCommand::Stop);
        self.worker
            .join()
            .map_err(|_| anyhow::anyhow!("Loopback fixture panicked"))?
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ParseFailure {
    // The failed document's script thread unwinds.
    ScriptThread,
    // The failed document's content process exits at a barrier. Multiprocess only.
    ProcessExit,
    // The host kills the failed document's content process at a barrier. Multiprocess only.
    ProcessKill,
}

impl ParseFailure {
    fn reason(self) -> &'static str {
        match self {
            Self::ScriptThread => "S1 probe actual script-thread panic",
            Self::ProcessExit | Self::ProcessKill => "Content process disconnected",
        }
    }

    fn how(self) -> &'static str {
        match self {
            Self::ScriptThread => "script-thread unwind",
            Self::ProcessExit => "child exit",
            Self::ProcessKill => "host SIGKILL",
        }
    }

    // Only the lifeline can report a process failure. Bound its delay.
    fn wait(self, deadline: Instant) -> Instant {
        match self {
            Self::ScriptThread => deadline,
            Self::ProcessExit | Self::ProcessKill => {
                deadline.min(Instant::now() + Duration::from_secs(5))
            }
        }
    }
}

fn held_count(delegate: &Delegate, path: &str) -> usize {
    delegate
        .held
        .borrow()
        .iter()
        .filter(|load| load.request().url.path() == path)
        .count()
}

// Release withheld scripts with late data. The failed document must not receive it.
fn release_held(delegate: &Delegate, path: &str, expected: usize) -> Result<()> {
    let held = delegate.held.take();
    ensure!(
        held.len() == expected && held.iter().all(|load| load.request().url.path() == path),
        "Unexpected withheld requests: {:?}",
        held.iter()
            .map(|load| load.request().url.to_string())
            .collect::<Vec<_>>()
    );
    for load in held {
        let url = load.request().url.clone();
        let mut late = load.intercept(WebResourceResponse::new(url));
        late.send_body_data(b"window.marker='late-script'".to_vec());
        late.finish();
    }
    Ok(())
}

// Lifecycle readiness events for one document, in order.
fn readiness(delegate: &Delegate, document: DocumentIdentity) -> Vec<&'static str> {
    delegate
        .events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            DocumentLifecycle::DomReady(id) if *id == document => Some("dom-ready"),
            DocumentLifecycle::LoadComplete(id) if *id == document => Some("load-complete"),
            _ => None,
        })
        .collect()
}

// A cross-site navigation's document parses in a separate event loop. Servo activates it when
// the script thread creates it at the response headers, so it is no longer pending while it
// parses. The source document stays in history in its own event loop with a popup.
#[allow(clippy::too_many_arguments)]
fn cross_site_parse_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    multiprocess: bool,
    survivor: (&WebView, DocumentIdentity),
    failure: ParseFailure,
) -> Result<String> {
    let reason = failure.reason();
    let held_path = "/held-cross-site-script";
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/cross-site-first"))?)
        .delegate(delegate.clone())
        .build();
    drive_until(servo, deadline, || delegate.loads.get() > 0)?;
    load(servo, &view, deadline, "/cross-site-source")?;
    let source = identity(servo, &view, deadline)?;
    // The popup's `about:blank` document shares the source document's event loop.
    let (popup, popup_delegate, popup_document) =
        open_popup(servo, &view, &delegate, context, deadline, source)?;
    let server = ParseServer::start(
        CROSS_SITE_PARSE,
        blocked_head(
            "<script>window.marker='/cross-site-parse'</script>",
            held_path,
        ),
        CROSS_SITE_RELOAD,
    )?;
    delegate.loopback_port.set(server.port);
    let origin = server.origin();
    view.load(url::Url::parse(&format!("{origin}{CROSS_SITE_PARSE}"))?);
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != source)
    })?;
    wait_marker(servo, &view, deadline, CROSS_SITE_PARSE)?;
    // Servo's prefetch tokenizer and the blocked parser each request the script.
    drive_until(servo, deadline, || held_count(&delegate, held_path) == 2)?;
    let failed = identity(servo, &view, deadline)?;
    ensure!(
        failed != source
            && failed.pipeline_id() != source.pipeline_id()
            && delegate.active.get() == Some(failed),
        "The cross-site document did not activate in a new pipeline"
    );
    text(
        scoped(
            servo,
            &view,
            deadline,
            failed,
            "document.readyState + ':' + (document.getElementById('after-script') === null)",
        )?,
        failed,
        "loading:true",
    )?;
    // No pending top-level pipeline remains while the document parses. With one, this
    // probe would unwind it and the case would fail.
    ensure!(
        view.crash_pending_script_thread_for_document_scope_probe()
            .is_none(),
        "A pending top-level pipeline remained while the document parsed"
    );
    if multiprocess {
        let failed_process = view.content_process_id_for_document_scope_probe();
        ensure!(
            failed_process != std::process::id()
                && failed_process != popup.content_process_id_for_document_scope_probe(),
            "The cross-site document shared the source event loop's process"
        );
    }
    let ticket = delegate.ticket(failed)?;
    let reply = scoped(
        servo,
        &view,
        deadline,
        failed,
        "'before-cross-site-failure'",
    )?;
    ensure!(reply.is_ok(), "Cross-site parse control failed");
    let popup_ticket = popup_delegate.ticket(popup_document)?;
    let popup_reply = scoped(servo, &popup, deadline, popup_document, "'popup-retained'")?;
    ensure!(popup_reply.is_ok(), "Popup control failed");
    let queue = || CleanupAudit::queue(&view, Some(failed), &delegate, true);
    let audit = match failure {
        // The constellation forwards both messages in order, so the unwind precedes these requests.
        ParseFailure::ScriptThread => {
            view.crash_script_thread_for_document_scope_probe();
            queue()
        }
        ParseFailure::ProcessExit => {
            view.with_terminating_content_process_for_document_scope_probe(queue)
        }
        ParseFailure::ProcessKill => {
            view.with_killed_content_process_for_document_scope_probe(queue)
        }
    };
    // Send no further work to the failed document. Only revocation can complete these callbacks.
    drive_until(servo, failure.wait(deadline), || {
        audit.calls.get() == 8 && delegate.crashed.get()
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "Cross-site parse {} did not revoke: {error}; active={:?}, calls={}, crashes={:?}",
            failure.how(),
            delegate.active.get(),
            audit.calls.get(),
            delegate.crash_reasons.borrow()
        )
    })?;
    audit.check()?;
    ensure!(
        delegate.crash_reasons.borrow().as_slice() == [reason],
        "Unexpected failure notifications: {:?}",
        delegate.crash_reasons.borrow()
    );
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0,
        "Cross-site parse failure retained callbacks"
    );
    delegate.reject(ticket, reply)?;
    // The popup in the source event loop keeps its scope and its retained reply.
    ensure!(
        popup_delegate.active.get() == Some(popup_document)
            && popup_delegate.generation.get() == popup_ticket.generation
            && !popup_delegate.crashed.get(),
        "The failure revoked a document in the source event loop"
    );
    ensure!(
        popup_delegate.deliver(popup_ticket, popup_reply),
        "The source event loop's retained reply lost its scope"
    );
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != failed)
    })?;
    let crash = identity(servo, &view, deadline)?;
    crash_page(servo, &view, deadline, crash, reason)?;
    // Late script and body data for the failed document must not reach any live document.
    let errors_before_release = engine_errors.0.borrow().len();
    release_held(&delegate, held_path, 2)?;
    server.release()?;
    let settle = Instant::now() + Duration::from_millis(300);
    drive_until(servo, deadline, || Instant::now() >= settle)?;
    ensure!(
        identity(servo, &view, deadline)? == crash,
        "Late response data replaced the crash document"
    );
    crash_page(servo, &view, deadline, crash, reason)?;
    ensure!(
        readiness(&delegate, failed).is_empty(),
        "The failed cross-site document reported readiness"
    );
    ensure!(
        delegate.crash_reasons.borrow().len() == 1,
        "Late response data caused another failure"
    );
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "Late response data raised engine errors: {late_errors:?}"
    );
    stale(scoped(servo, &view, deadline, failed, "'obsolete'")?)?;
    stale(scoped(servo, &view, deadline, source, "'obsolete'")?)?;
    // Reload restores the failed load. The second response is complete.
    delegate.serve_held.set(true);
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != crash)
    })?;
    wait_marker(servo, &view, deadline, CROSS_SITE_PARSE)?;
    let reloaded = identity(servo, &view, deadline)?;
    ensure!(
        reloaded != failed,
        "Reload revived the failed cross-site scope"
    );
    text(
        scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
        reloaded,
        &origin,
    )?;
    // Recovery closed the source document's history pipeline in the healthy event loop.
    // Back must rebuild it with a fresh identity.
    let traversal = view.go_back(1);
    drive_until(servo, deadline, || {
        delegate.traversals.borrow().contains(&traversal)
    })?;
    wait_marker(servo, &view, deadline, "/cross-site-source")?;
    let back = identity(servo, &view, deadline)?;
    ensure!(
        back != source,
        "Back revived the closed source document's scope"
    );
    text(
        scoped(servo, &view, deadline, back, "String(window.origin)")?,
        back,
        ORIGIN,
    )?;
    stale(scoped(servo, &view, deadline, source, "'obsolete'")?)?;
    load(servo, &view, deadline, "/after-cross-site-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(servo, &view, deadline, recovered, "'cross-site-recovered'")?,
        recovered,
        "cross-site-recovered",
    )?;
    text(
        scoped(servo, &popup, deadline, popup_document, "'popup-survivor'")?,
        popup_document,
        "popup-survivor",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'cross-site-survivor'",
        )?,
        survivor_document,
        "cross-site-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "Cross-site parse recovery repeated callbacks"
    );
    let served = server.finish()?;
    ensure!(
        served.paths == [CROSS_SITE_PARSE, CROSS_SITE_PARSE],
        "Unexpected loopback requests: {:?}",
        served.paths
    );
    Ok(format!(
        "{}, separate event loop, no pending pipeline during parse, 8 callbacks released, source-loop popup kept scope, late data ignored, tail_delivered={:?}, reload, back, and new URL recover, survivor usable",
        failure.how(),
        served.tail_delivered
    ))
}

#[derive(Clone, Copy, PartialEq)]
enum FrameParse {
    // A cross-site child frame waits for a withheld script in a loopback response.
    CrossSite,
    // A sandboxed `srcdoc` child has an opaque origin, so Servo never reuses an event loop for it.
    SandboxedSrcdoc,
}

impl FrameParse {
    fn parent_path(self) -> &'static str {
        match self {
            Self::CrossSite => "/frame-parent",
            Self::SandboxedSrcdoc => "/sandboxed-srcdoc-parent",
        }
    }

    fn held_path(self) -> &'static str {
        match self {
            Self::CrossSite => "/held-frame-script",
            Self::SandboxedSrcdoc => "/held-sandboxed-script",
        }
    }

    // The prefetch tokenizer also requests the script of a fetched document.
    fn held_requests(self) -> usize {
        match self {
            Self::CrossSite => 2,
            Self::SandboxedSrcdoc => 1,
        }
    }
}

// The constellation's record of the active top-level document's newest child frame: its
// current pipeline, whether that pipeline is live, its crash details, and whether the frame
// has a pending pipeline. Page JavaScript cannot supply these values.
fn frame_state(
    view: &WebView,
) -> Result<(
    impl Copy + PartialEq + std::fmt::Debug,
    bool,
    Option<String>,
    bool,
)> {
    view.frame_state_for_document_scope_probe()
        .ok_or_else(|| anyhow::anyhow!("The parent has no child frame"))
}

// Drive the engine for a fixed time, so queued engine messages reach the host.
fn settle(servo: &Servo, deadline: Instant, milliseconds: u64) -> Result<()> {
    let settle = Instant::now() + Duration::from_millis(milliseconds);
    drive_until(servo, deadline, || Instant::now() >= settle)
}

// Fail the event loop of a child frame while its parser waits for a withheld script.
// The parent runs in another event loop. One popup shares the parent's event loop, and for a
// cross-site child, another popup loads the child's site and shares the child's event loop.
#[allow(clippy::too_many_arguments)]
fn frame_parse_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    multiprocess: bool,
    survivor: (&WebView, DocumentIdentity),
    frame: FrameParse,
    failure: ParseFailure,
) -> Result<String> {
    let reason = failure.reason();
    let parent_path = frame.parent_path();
    let held_path = frame.held_path();
    let server = match frame {
        FrameParse::CrossSite => Some(ParseServer::start(
            PARSE_FRAME,
            blocked_head(FRAME_PARSING, held_path),
            FRAME_RELOAD,
        )?),
        FrameParse::SandboxedSrcdoc => None,
    };
    let markup = match &server {
        Some(server) => format!(
            "{FRAME_LISTENER}<iframe src='{}{PARSE_FRAME}'></iframe>",
            server.origin()
        ),
        None => format!(
            "{FRAME_LISTENER}<iframe sandbox='allow-scripts' srcdoc=\"{FRAME_PARSING}<script src='{held_path}'></script><p id='after-script'>\"></iframe>"
        ),
    };
    let delegate = Rc::new(Delegate::default());
    *delegate.frame_markup.borrow_mut() = Some((parent_path.into(), markup));
    if let Some(server) = &server {
        delegate.loopback_port.set(server.port);
    }
    let view = WebViewBuilder::new(servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}{parent_path}"))?)
        .delegate(delegate.clone())
        .build();
    // The parent's load event waits for the child, so wait for activation and the marker.
    drive_until(servo, deadline, || delegate.active.get().is_some())?;
    wait_marker(servo, &view, deadline, parent_path)?;
    let parent = identity(servo, &view, deadline)?;
    wait_text(
        servo,
        &view,
        deadline,
        parent,
        "String(window.frameState)",
        "frame-parsing",
    )?;
    drive_until(servo, deadline, || {
        held_count(&delegate, held_path) == frame.held_requests()
    })?;
    // The parent finishes parsing, but its load event waits for the blocked child.
    drive_until(servo, deadline, || !readiness(&delegate, parent).is_empty())?;
    ensure!(
        readiness(&delegate, parent) == ["dom-ready"],
        "Unexpected parent readiness before the failure: {:?}",
        readiness(&delegate, parent)
    );
    // The parent cannot reach the child's document synchronously, unlike a same-thread srcdoc.
    text(
        scoped(
            servo,
            &view,
            deadline,
            parent,
            "document.readyState + ':' + frames.length + ':' + (() => { try { return String(frames[0].document); } catch (error) { return error.name; } })()",
        )?,
        parent,
        "interactive:1:SecurityError",
    )?;
    // This popup's `about:blank` document shares the parent's event loop.
    let (parent_loop, parent_loop_delegate, parent_loop_document) =
        open_popup(servo, &view, &delegate, context.clone(), deadline, parent)?;
    // This popup loads the child's site, so it shares the child's event loop.
    let child_loop = match &server {
        Some(server) => {
            let (popup, popup_delegate, _) =
                open_popup(servo, &view, &delegate, context.clone(), deadline, parent)?;
            popup_delegate.loopback_port.set(server.port);
            popup.load(url::Url::parse(&format!(
                "{}{FRAME_SIBLING}",
                server.origin()
            ))?);
            wait_marker(servo, &popup, deadline, FRAME_SIBLING)?;
            let document = identity(servo, &popup, deadline)?;
            Some((popup, popup_delegate, document))
        }
        None => None,
    };
    let processes = if multiprocess {
        let parent_process = view.content_process_id_for_document_scope_probe();
        let frame_process = view.frame_content_process_id_for_document_scope_probe();
        ensure!(
            frame_process != parent_process
                && frame_process != std::process::id()
                && parent_loop.content_process_id_for_document_scope_probe() == parent_process,
            "The child frame did not run in its own content process"
        );
        if let Some((popup, ..)) = &child_loop {
            ensure!(
                popup.content_process_id_for_document_scope_probe() == frame_process,
                "The child-site popup did not share the child frame's content process"
            );
        }
        Some((parent_process, frame_process))
    } else {
        None
    };
    // The constellation's record of the child frame before the failure.
    let before = frame_state(&view)?;
    ensure!(
        before.1 && before.2.is_none() && !before.3,
        "Unexpected child frame state before the failure: {before:?}"
    );
    let ticket = delegate.ticket(parent)?;
    let reply = scoped(servo, &view, deadline, parent, "'before-frame-failure'")?;
    ensure!(reply.is_ok(), "Frame parse control failed");
    let parent_loop_ticket = parent_loop_delegate.ticket(parent_loop_document)?;
    let parent_loop_reply = scoped(
        servo,
        &parent_loop,
        deadline,
        parent_loop_document,
        "'parent-loop-retained'",
    )?;
    ensure!(
        parent_loop_reply.is_ok(),
        "Parent-loop popup control failed"
    );
    let child_loop_retained = match &child_loop {
        Some((popup, popup_delegate, document)) => {
            let ticket = popup_delegate.ticket(*document)?;
            let reply = scoped(servo, popup, deadline, *document, "'child-loop-retained'")?;
            ensure!(reply.is_ok(), "Child-loop popup control failed");
            Some((ticket, reply))
        }
        None => None,
    };
    let mut separate = None;
    // Hold the healthy parent's script thread. Its callbacks cannot complete, and the frame
    // cannot navigate, until the constellation has handled the failure.
    let (audit, child_loop_audit, handled, held) = view
        .with_paused_script_for_document_scope_probe(|| {
            let audit = CleanupAudit::queue(&view, Some(parent), &delegate, true);
            let queue_child_loop = || {
                child_loop
                    .as_ref()
                    .map(|(popup, popup_delegate, document)| {
                        CleanupAudit::queue(popup, Some(*document), popup_delegate, true)
                    })
            };
            let child_loop_audit = match failure {
                ParseFailure::ScriptThread => {
                    separate = view.crash_frame_script_thread_for_document_scope_probe();
                    queue_child_loop()
                }
                ParseFailure::ProcessExit => view
                    .with_terminating_frame_content_process_for_document_scope_probe(
                        queue_child_loop,
                    ),
                ParseFailure::ProcessKill => view
                    .with_killed_frame_content_process_for_document_scope_probe(queue_child_loop),
            };
            let barrier = deadline.min(Instant::now() + Duration::from_secs(4));
            // The failed frame pipeline is gone, and each view in the failed event loop is revoked.
            // A revocation of the parent also ends this wait, so the check below reports it.
            let handled = drive_until(servo, barrier, || {
                child_loop_audit
                    .as_ref()
                    .is_none_or(|audit| audit.calls.get() == 8)
                    && (frame_state(&view).is_ok_and(|state| state.0 == before.0 && !state.1)
                        || audit.calls.get() > 0
                        || delegate.crashed.get())
            })
            .and_then(|()| settle(servo, barrier, 200));
            let held = (
                audit.calls.get(),
                delegate.active.get(),
                delegate.crash_reasons.borrow().clone(),
                frame_state(&view).ok(),
            );
            (audit, child_loop_audit, handled, held)
        });
    if failure == ParseFailure::ScriptThread {
        ensure!(
            separate == Some(true),
            "Probe did not reach a child frame in a separate event loop: {separate:?}"
        );
    }
    handled.map_err(|error| {
        anyhow::anyhow!(
            "Child frame {} was not handled within the barrier: {error}; frame={:?}, calls={:?}",
            failure.how(),
            frame_state(&view).ok(),
            child_loop_audit.as_ref().map(|audit| audit.calls.get()),
        )
    })?;
    // The failure stays inside the frame. The parent is not revoked while its thread waits.
    let (calls, active, crashes, frame_held) = held;
    ensure!(
        (calls, active, crashes.as_slice()) == (0, Some(parent), &[][..]),
        "The child frame {} revoked the healthy parent: calls={calls}, active={active:?}, crashes={crashes:?}",
        failure.how()
    );
    // The failed frame pipeline is closed. The frame cannot navigate while the parent waits.
    ensure!(
        frame_held
            .as_ref()
            .is_some_and(|state| state.0 == before.0 && !state.1 && !state.3),
        "The failed frame pipeline was not closed: {frame_held:?}"
    );
    // The parent's held callbacks then complete in the same document.
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check_completed(parent)?;
    if let Some(audit) = &child_loop_audit {
        audit.check()?;
    }
    // The child-site popup's own top-level document ran in the failed event loop.
    if let (Some((popup, popup_delegate, _)), Some((ticket, reply))) =
        (&child_loop, child_loop_retained)
    {
        drive_until(servo, deadline, || popup_delegate.crashed.get())?;
        ensure!(
            popup_delegate.crash_reasons.borrow().as_slice() == [reason]
                && popup.pending_evaluations_for_document_scope_probe() == 0,
            "Unexpected child-loop popup failure notifications: {:?}",
            popup_delegate.crash_reasons.borrow()
        );
        popup_delegate.reject(ticket, reply)?;
    }
    // The popup in the parent's event loop keeps its scope and its retained reply.
    ensure!(
        parent_loop_delegate.active.get() == Some(parent_loop_document)
            && parent_loop_delegate.generation.get() == parent_loop_ticket.generation
            && !parent_loop_delegate.crashed.get(),
        "The failure revoked a document in the parent's event loop"
    );
    ensure!(
        parent_loop_delegate.deliver(parent_loop_ticket, parent_loop_reply),
        "The parent loop's retained reply lost its scope"
    );
    // Only the failed frame shows an error state. Its parent navigates it to the crash page.
    let mut crashed_frame = None;
    drive_until(servo, deadline, || {
        crashed_frame = frame_state(&view)
            .ok()
            .filter(|state| state.0 != before.0 && state.1 && !state.3);
        crashed_frame.is_some()
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "The failed frame did not activate a new pipeline: {error}; frame={:?}",
            frame_state(&view).ok()
        )
    })?;
    let crashed_frame = crashed_frame.unwrap();
    ensure!(
        crashed_frame
            .2
            .as_deref()
            .is_some_and(|crash| crash.starts_with(reason)),
        "The failed frame did not get a crash page: {crashed_frame:?}"
    );
    // The parent keeps its document, its scope, and its retained reply. Its load event no
    // longer waits for the failed child.
    drive_until(servo, deadline, || {
        readiness(&delegate, parent).contains(&"load-complete")
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "The parent's load did not complete after the frame's crash page: {error}; readiness={:?}, held={}",
            readiness(&delegate, parent),
            delegate.held.borrow().len()
        )
    })?;
    ensure!(
        readiness(&delegate, parent) == ["dom-ready", "load-complete"]
            && delegate.active.get() == Some(parent)
            && delegate.generation.get() == ticket.generation
            && !delegate.crashed.get()
            && view.pending_evaluations_for_document_scope_probe() == 0,
        "The parent did not keep its document: readiness={:?}, active={:?}, crashes={:?}",
        readiness(&delegate, parent),
        delegate.active.get(),
        delegate.crash_reasons.borrow()
    );
    ensure!(
        delegate.deliver(ticket, reply),
        "The parent's retained reply lost its scope"
    );
    // The crash page has an opaque origin. The parent still cannot read the frame's document.
    let frame_view = "String(window.frameState) + ':' + frames.length + ':' + (() => { try { return String(frames[0].document); } catch (error) { return error.name; } })()";
    text(
        scoped(servo, &view, deadline, parent, frame_view)?,
        parent,
        "frame-parsing:1:SecurityError",
    )?;
    if let Some((parent_process, frame_process)) = processes {
        let crash_process = view.frame_content_process_id_for_document_scope_probe();
        ensure!(
            crash_process != parent_process
                && crash_process != frame_process
                && crash_process != std::process::id(),
            "The frame's crash page did not run in a new content process"
        );
    }
    let child_loop_crash = match &child_loop {
        Some((popup, popup_delegate, document)) => {
            drive_until(servo, deadline, || {
                popup_delegate
                    .active
                    .get()
                    .is_some_and(|id| id != *document)
            })?;
            let crash = identity(servo, popup, deadline)?;
            crash_page(servo, popup, deadline, crash, reason)?;
            Some(crash)
        }
        None => None,
    };
    // Late script and body data for the failed child must not reach any live document.
    let errors_before_release = engine_errors.0.borrow().len();
    release_held(&delegate, held_path, frame.held_requests())?;
    if let Some(server) = &server {
        server.release()?;
    }
    settle(servo, deadline, 300)?;
    ensure!(
        identity(servo, &view, deadline)? == parent
            && frame_state(&view)? == crashed_frame
            && readiness(&delegate, parent) == ["dom-ready", "load-complete"],
        "Late child data changed the parent or the frame's crash page"
    );
    text(
        scoped(servo, &view, deadline, parent, frame_view)?,
        parent,
        "frame-parsing:1:SecurityError",
    )?;
    ensure!(
        delegate.crash_reasons.borrow().is_empty()
            && child_loop
                .as_ref()
                .is_none_or(
                    |(_, popup_delegate, _)| popup_delegate.crash_reasons.borrow().len() == 1
                ),
        "Late child data caused another failure"
    );
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "Late child data raised engine errors: {late_errors:?}"
    );
    // Reload replaces the parent, and the child then completes.
    delegate.serve_held.set(true);
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != parent)
    })?;
    let reloaded = identity(servo, &view, deadline)?;
    ensure!(reloaded != parent, "Reload kept the parent scope");
    stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
    text(
        scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
        reloaded,
        ORIGIN,
    )?;
    drive_until(servo, deadline, || {
        readiness(&delegate, reloaded).contains(&"load-complete")
    })?;
    wait_text(
        servo,
        &view,
        deadline,
        reloaded,
        "String(window.frameState)",
        match frame {
            FrameParse::CrossSite => "frame-complete",
            FrameParse::SandboxedSrcdoc => "frame-parsing",
        },
    )?;
    ensure!(
        frame_state(&view)?.2.is_none(),
        "Reload kept the frame's crash page"
    );
    load(servo, &view, deadline, "/after-frame-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(servo, &view, deadline, recovered, "'frame-recovered'")?,
        recovered,
        "frame-recovered",
    )?;
    if let (Some((popup, _, document)), Some(crash)) = (&child_loop, child_loop_crash) {
        stale(scoped(servo, popup, deadline, *document, "'obsolete'")?)?;
        load(servo, popup, deadline, "/after-frame-sibling-failure")?;
        let popup_recovered = identity(servo, popup, deadline)?;
        ensure!(
            popup_recovered != crash,
            "Child-loop popup did not navigate"
        );
        text(
            scoped(
                servo,
                popup,
                deadline,
                popup_recovered,
                "'child-loop-recovered'",
            )?,
            popup_recovered,
            "child-loop-recovered",
        )?;
    }
    text(
        scoped(
            servo,
            &parent_loop,
            deadline,
            parent_loop_document,
            "'parent-loop-survivor'",
        )?,
        parent_loop_document,
        "parent-loop-survivor",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'frame-survivor'",
        )?,
        survivor_document,
        "frame-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8
            && child_loop_audit
                .as_ref()
                .is_none_or(|audit| audit.calls.get() == 8),
        "Frame failure recovery repeated callbacks"
    );
    let tail = match server {
        Some(server) => {
            let served = server.finish()?;
            ensure!(
                served.paths == [PARSE_FRAME, FRAME_SIBLING, PARSE_FRAME],
                "Unexpected loopback requests: {:?}",
                served.paths
            );
            format!(", tail_delivered={:?}", served.tail_delivered)
        }
        None => String::new(),
    };
    Ok(format!(
        "{}, child in a separate event loop, parent kept its document, scope, and retained reply, 8 held parent callbacks completed, frame crash page, parent load completed, {}, late data ignored{tail}, reload and new URL recover, survivor usable",
        failure.how(),
        match frame {
            FrameParse::CrossSite => {
                "child-site popup revoked before 8 callbacks, parent-loop popup kept scope"
            }
            FrameParse::SandboxedSrcdoc => "parent-loop popup kept scope",
        }
    ))
}

// Pending child-frame navigation fixtures. The host withholds the frame's navigation response,
// so its new pipeline exists without a document while the old child document stays active.
const PENDING_FRAME_CHILD: &str = "/pending-frame-child";
const PENDING_FRAME_NAVIGATION: &str = "/held-frame-navigation";

#[derive(Clone, Copy, PartialEq)]
enum FrameSite {
    // The pending pipeline shares the parent's event loop.
    SameSite,
    // The pending pipeline joins the event loop of a popup at the child's site.
    CrossSite,
    // The pending pipeline is the only pipeline in a new event loop.
    CrossSiteAlone,
}

impl FrameSite {
    fn parent_path(self) -> &'static str {
        match self {
            Self::SameSite => "/same-site-pending-parent",
            Self::CrossSite => "/cross-site-pending-parent",
            Self::CrossSiteAlone => "/cross-site-lone-pending-parent",
        }
    }
}

// Answer a withheld navigation with a document. It must not reach any live document.
fn release_held_document(delegate: &Delegate, path: &str) -> Result<()> {
    let held = delegate.held.take();
    ensure!(
        held.len() == 1 && held[0].request().url.path() == path,
        "Unexpected withheld requests: {:?}",
        held.iter()
            .map(|load| load.request().url.to_string())
            .collect::<Vec<_>>()
    );
    for load in held {
        let url = load.request().url.clone();
        let mut response = WebResourceResponse::new(url);
        response
            .headers
            .insert("content-type", "text/html; charset=utf-8".parse().unwrap());
        let mut late = load.intercept(response);
        late.send_body_data(
            b"<!doctype html><script>window.marker='late-navigation';parent.postMessage('late-navigation', '*')</script>"
                .to_vec(),
        );
        late.finish();
    }
    Ok(())
}

// Fail the event loop of a child frame's pending navigation before its response headers.
// The parent has one popup in its own event loop. For `CrossSite`, a second popup loads the
// child's site first, so the pending pipeline joins that popup's event loop.
#[allow(clippy::too_many_arguments)]
fn frame_pending_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    multiprocess: bool,
    survivor: (&WebView, DocumentIdentity),
    site: FrameSite,
    failure: ParseFailure,
) -> Result<String> {
    let reason = failure.reason();
    let parent_path = site.parent_path();
    let server = match site {
        // The host withholds the frame's navigation before it reaches the network, so this
        // server only serves the child-site popup.
        FrameSite::CrossSite => Some(ParseServer::start(
            PENDING_FRAME_NAVIGATION,
            String::new(),
            "",
        )?),
        FrameSite::SameSite | FrameSite::CrossSiteAlone => None,
    };
    let target = match (&server, site) {
        (Some(server), _) => format!("{}{PENDING_FRAME_NAVIGATION}", server.origin()),
        (None, FrameSite::CrossSiteAlone) => {
            format!("https://pending-frame.invalid{PENDING_FRAME_NAVIGATION}")
        }
        (None, _) => PENDING_FRAME_NAVIGATION.to_owned(),
    };
    let delegate = Rc::new(Delegate::default());
    *delegate.frame_markup.borrow_mut() = Some((
        parent_path.into(),
        format!("{FRAME_LISTENER}<iframe src='{PENDING_FRAME_CHILD}'></iframe>"),
    ));
    let view = WebViewBuilder::new(servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}{parent_path}"))?)
        .delegate(delegate.clone())
        .build();
    drive_until(servo, deadline, || delegate.active.get().is_some())?;
    wait_marker(servo, &view, deadline, parent_path)?;
    let parent = identity(servo, &view, deadline)?;
    drive_until(servo, deadline, || {
        readiness(&delegate, parent).contains(&"load-complete")
    })?;
    // A cross-origin document throws on access. That shows replacement, not its content.
    let child_marker = "String(frames.length) + ':' + (() => { try { return String(frames[0].marker); } catch (error) { return error.name; } })()";
    let child_loaded = format!("1:{PENDING_FRAME_CHILD}");
    text(
        scoped(servo, &view, deadline, parent, child_marker)?,
        parent,
        &child_loaded,
    )?;
    // This popup's `about:blank` document shares the parent's event loop.
    let (parent_loop, parent_loop_delegate, parent_loop_document) =
        open_popup(servo, &view, &delegate, context.clone(), deadline, parent)?;
    // This popup loads the child's site before the frame navigates, so the pending pipeline
    // joins the popup's event loop.
    let child_loop = match &server {
        Some(server) => {
            let (popup, popup_delegate, _) =
                open_popup(servo, &view, &delegate, context.clone(), deadline, parent)?;
            popup_delegate.loopback_port.set(server.port);
            popup.load(url::Url::parse(&format!(
                "{}{FRAME_SIBLING}",
                server.origin()
            ))?);
            wait_marker(servo, &popup, deadline, FRAME_SIBLING)?;
            let document = identity(servo, &popup, deadline)?;
            Some((popup, popup_delegate, document))
        }
        None => None,
    };
    text(
        scoped(
            servo,
            &view,
            deadline,
            parent,
            &format!("document.querySelector('iframe').src = {target:?}; 'navigating'"),
        )?,
        parent,
        "navigating",
    )?;
    drive_until(servo, deadline, || {
        held_count(&delegate, PENDING_FRAME_NAVIGATION) == 1
    })?;
    // The old child document stays active while the navigation waits for its response.
    text(
        scoped(servo, &view, deadline, parent, child_marker)?,
        parent,
        &child_loaded,
    )?;
    ensure!(
        delegate.active.get() == Some(parent)
            && readiness(&delegate, parent) == ["dom-ready", "load-complete"],
        "The pending frame navigation changed the parent's lifecycle"
    );
    // The pending pipeline belongs to the child frame, not to the top level. With a pending
    // top-level pipeline, this probe would unwind it and the case would fail.
    ensure!(
        view.crash_pending_script_thread_for_document_scope_probe()
            .is_none(),
        "The frame navigation created a pending top-level pipeline"
    );
    let processes = if multiprocess {
        let host = std::process::id();
        let parent_process = view.content_process_id_for_document_scope_probe();
        let frame_process = view.frame_content_process_id_for_document_scope_probe();
        let pending_process = view.pending_frame_content_process_id_for_document_scope_probe();
        ensure!(
            parent_process != host
                && frame_process == parent_process
                && parent_loop.content_process_id_for_document_scope_probe() == parent_process,
            "The parent, its active child, and the parent-loop popup did not share one process"
        );
        match (site, &child_loop) {
            (FrameSite::SameSite, _) => ensure!(
                pending_process == parent_process,
                "The same-site pending pipeline did not share the parent's process"
            ),
            (_, Some((popup, ..))) => ensure!(
                pending_process != parent_process
                    && pending_process != host
                    && popup.content_process_id_for_document_scope_probe() == pending_process,
                "The cross-site pending pipeline did not share only the child-site popup's process"
            ),
            (_, None) => ensure!(
                pending_process != parent_process
                    && pending_process != host
                    && survivor.0.content_process_id_for_document_scope_probe() != pending_process,
                "The lone pending pipeline did not run in a process of its own"
            ),
        }
        Some((parent_process, pending_process))
    } else {
        None
    };
    // The constellation's record of the frame: the old child is current and live, and the
    // navigation is a pending pipeline.
    let before = frame_state(&view)?;
    ensure!(
        before.1 && before.2.is_none() && before.3,
        "Unexpected child frame state before the failure: {before:?}"
    );
    let ticket = delegate.ticket(parent)?;
    let reply = scoped(
        servo,
        &view,
        deadline,
        parent,
        "'before-pending-frame-failure'",
    )?;
    ensure!(reply.is_ok(), "Pending frame control failed");
    let parent_loop_ticket = parent_loop_delegate.ticket(parent_loop_document)?;
    let parent_loop_reply = scoped(
        servo,
        &parent_loop,
        deadline,
        parent_loop_document,
        "'parent-loop-retained'",
    )?;
    ensure!(
        parent_loop_reply.is_ok(),
        "Parent-loop popup control failed"
    );
    let child_loop_retained = match &child_loop {
        Some((popup, popup_delegate, document)) => {
            let ticket = popup_delegate.ticket(*document)?;
            let reply = scoped(servo, popup, deadline, *document, "'child-loop-retained'")?;
            ensure!(reply.is_ok(), "Child-loop popup control failed");
            Some((ticket, reply))
        }
        None => None,
    };
    let mut separate = None;
    if site == FrameSite::SameSite {
        // The pending pipeline shares the parent's event loop, so the parent's own top-level
        // document fails with it. The constellation forwards the unwind before these requests,
        // and the barrier holds them in the child before exit or kill.
        let queue_both = || {
            (
                CleanupAudit::queue(&view, Some(parent), &delegate, true),
                CleanupAudit::queue(
                    &parent_loop,
                    Some(parent_loop_document),
                    &parent_loop_delegate,
                    true,
                ),
            )
        };
        let (audit, popup_audit) = match failure {
            ParseFailure::ScriptThread => {
                separate = view.crash_pending_frame_script_thread_for_document_scope_probe();
                queue_both()
            }
            ParseFailure::ProcessExit => view
                .with_terminating_pending_frame_content_process_for_document_scope_probe(
                    queue_both,
                ),
            ParseFailure::ProcessKill => {
                view.with_killed_pending_frame_content_process_for_document_scope_probe(queue_both)
            }
        };
        ensure!(
            failure != ParseFailure::ScriptThread || separate == Some(false),
            "Probe did not reach the pending frame pipeline in the parent's event loop: {separate:?}"
        );
        drive_until(servo, failure.wait(deadline), || {
            audit.calls.get() == 8 && popup_audit.calls.get() == 8
        })
        .map_err(|error| {
            anyhow::anyhow!(
                "Pending frame {} did not revoke the parent: {error}; active={:?}, calls={}, crashes={:?}",
                failure.how(),
                delegate.active.get(),
                audit.calls.get(),
                delegate.crash_reasons.borrow()
            )
        })?;
        audit.check()?;
        popup_audit.check()?;
        drive_until(servo, deadline, || {
            delegate.crashed.get() && parent_loop_delegate.crashed.get()
        })?;
        for (name, popup, popup_delegate) in [
            ("parent", &view, &delegate),
            ("parent-loop popup", &parent_loop, &parent_loop_delegate),
        ] {
            ensure!(
                popup_delegate.crash_reasons.borrow().as_slice() == [reason]
                    && popup.pending_evaluations_for_document_scope_probe() == 0,
                "Unexpected {name} failure notifications: {:?}",
                popup_delegate.crash_reasons.borrow()
            );
        }
        delegate.reject(ticket, reply)?;
        parent_loop_delegate.reject(parent_loop_ticket, parent_loop_reply)?;
        // Upstream recovery replaces each failed view with a crash document.
        drive_until(servo, deadline, || {
            delegate.active.get().is_some_and(|id| id != parent)
        })?;
        let crash = identity(servo, &view, deadline)?;
        crash_page(servo, &view, deadline, crash, reason)?;
        drive_until(servo, deadline, || {
            parent_loop_delegate
                .active
                .get()
                .is_some_and(|id| id != parent_loop_document)
        })?;
        let popup_crash = identity(servo, &parent_loop, deadline)?;
        // Servo loads `about:blank` without the crash fetch, so this crash document is blank.
        let blank_crash = format!(
            "String(location.href) + '|' + document.documentElement.textContent.includes({reason:?})"
        );
        text(
            scoped(servo, &parent_loop, deadline, popup_crash, &blank_crash)?,
            popup_crash,
            "about:blank|false",
        )?;
        // A late navigation response for the failed pipeline must not reach any live document.
        let errors_before_release = engine_errors.0.borrow().len();
        release_held_document(&delegate, PENDING_FRAME_NAVIGATION)?;
        settle(servo, deadline, 300)?;
        ensure!(
            identity(servo, &view, deadline)? == crash
                && identity(servo, &parent_loop, deadline)? == popup_crash,
            "The late navigation response replaced a crash document"
        );
        crash_page(servo, &view, deadline, crash, reason)?;
        text(
            scoped(servo, &parent_loop, deadline, popup_crash, &blank_crash)?,
            popup_crash,
            "about:blank|false",
        )?;
        ensure!(
            readiness(&delegate, parent) == ["dom-ready", "load-complete"],
            "The failed parent reported readiness after the failure"
        );
        ensure!(
            delegate.crash_reasons.borrow().len() == 1
                && parent_loop_delegate.crash_reasons.borrow().len() == 1,
            "The late navigation response caused another failure"
        );
        let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
        ensure!(
            late_errors.is_empty(),
            "The late navigation response raised engine errors: {late_errors:?}"
        );
        stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
        stale(scoped(
            servo,
            &parent_loop,
            deadline,
            parent_loop_document,
            "'obsolete'",
        )?)?;
        // Reload restores the parent's load, and its child frame loads its original document.
        view.reload();
        drive_until(servo, deadline, || {
            delegate.active.get().is_some_and(|id| id != crash)
        })?;
        wait_marker(servo, &view, deadline, parent_path)?;
        let reloaded = identity(servo, &view, deadline)?;
        ensure!(reloaded != parent, "Reload revived the failed parent scope");
        text(
            scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
            reloaded,
            ORIGIN,
        )?;
        drive_until(servo, deadline, || {
            readiness(&delegate, reloaded).contains(&"load-complete")
        })?;
        text(
            scoped(servo, &view, deadline, reloaded, child_marker)?,
            reloaded,
            &child_loaded,
        )?;
        load(servo, &view, deadline, "/after-pending-frame-failure")?;
        let recovered = identity(servo, &view, deadline)?;
        text(
            scoped(
                servo,
                &view,
                deadline,
                recovered,
                "'pending-frame-recovered'",
            )?,
            recovered,
            "pending-frame-recovered",
        )?;
        load(servo, &parent_loop, deadline, "/after-pending-frame-popup")?;
        let popup_recovered = identity(servo, &parent_loop, deadline)?;
        ensure!(
            popup_recovered != popup_crash && popup_recovered != parent_loop_document,
            "The revoked popup did not navigate"
        );
        text(
            scoped(
                servo,
                &parent_loop,
                deadline,
                popup_recovered,
                "'popup-recovered'",
            )?,
            popup_recovered,
            "popup-recovered",
        )?;
        let (survivor, survivor_document) = survivor;
        text(
            scoped(
                servo,
                survivor,
                deadline,
                survivor_document,
                "'pending-frame-survivor'",
            )?,
            survivor_document,
            "pending-frame-survivor",
        )?;
        ensure!(
            audit.calls.get() == 8 && popup_audit.calls.get() == 8,
            "Pending frame recovery repeated callbacks"
        );
        return Ok(format!(
            "{}, pending child pipeline in the parent's event loop, old child stayed active, parent's own document failed, parent revoked before 8 callbacks and replaced, parent-loop popup revoked before 8 callbacks and recovered, late navigation response ignored, reload and new URL recover, survivor usable",
            failure.how(),
        ));
    }
    // The parent's event loop stays healthy. Hold its script thread, so its callbacks cannot
    // complete, and the frame cannot navigate, until the constellation has handled the failure.
    let (audit, popup_audit, handled, held) =
        view.with_paused_script_for_document_scope_probe(|| {
            let audit = CleanupAudit::queue(&view, Some(parent), &delegate, true);
            let queue_popup = || {
                child_loop
                    .as_ref()
                    .map(|(popup, popup_delegate, document)| {
                        CleanupAudit::queue(popup, Some(*document), popup_delegate, true)
                    })
            };
            let popup_audit = match failure {
                ParseFailure::ScriptThread => {
                    separate = view.crash_pending_frame_script_thread_for_document_scope_probe();
                    queue_popup()
                }
                ParseFailure::ProcessExit => view
                    .with_terminating_pending_frame_content_process_for_document_scope_probe(
                        queue_popup,
                    ),
                ParseFailure::ProcessKill => view
                    .with_killed_pending_frame_content_process_for_document_scope_probe(
                        queue_popup,
                    ),
            };
            let barrier = deadline.min(Instant::now() + Duration::from_secs(4));
            // The pending pipeline is gone, the old child is still current, and each view in
            // the failed event loop is revoked.
            // A revocation of the parent also ends this wait, so the check below reports it.
            let handled = drive_until(servo, barrier, || {
                popup_audit
                    .as_ref()
                    .is_none_or(|audit| audit.calls.get() == 8)
                    && (frame_state(&view)
                        .is_ok_and(|state| state.0 == before.0 && state.1 && !state.3)
                        || audit.calls.get() > 0
                        || delegate.crashed.get())
            })
            .and_then(|()| settle(servo, barrier, 200));
            let held = (
                audit.calls.get(),
                delegate.active.get(),
                delegate.crash_reasons.borrow().clone(),
                frame_state(&view).ok(),
            );
            (audit, popup_audit, handled, held)
        });
    ensure!(
        failure != ParseFailure::ScriptThread || separate == Some(true),
        "Probe did not reach the pending frame pipeline in a separate event loop: {separate:?}"
    );
    handled.map_err(|error| {
        anyhow::anyhow!(
            "Pending frame {} was not handled within the barrier: {error}; frame={:?}, calls={:?}",
            failure.how(),
            frame_state(&view).ok(),
            popup_audit.as_ref().map(|audit| audit.calls.get()),
        )
    })?;
    // The failure stays inside the frame. The parent is not revoked while its thread waits.
    let (calls, active, crashes, frame_held) = held;
    ensure!(
        (calls, active, crashes.as_slice()) == (0, Some(parent), &[][..]),
        "The pending frame {} revoked the healthy parent: calls={calls}, active={active:?}, crashes={crashes:?}",
        failure.how()
    );
    // The pending pipeline is closed, and the old child is still current. The frame cannot
    // navigate while the parent waits.
    ensure!(
        frame_held
            .as_ref()
            .is_some_and(|state| state.0 == before.0 && state.1 && !state.3),
        "The pending frame pipeline was not closed: {frame_held:?}"
    );
    // The parent's held callbacks then complete in the same document.
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check_completed(parent)?;
    if let Some(audit) = &popup_audit {
        audit.check()?;
    }
    // The child-site popup's own top-level document ran in the failed event loop.
    if let (Some((popup, popup_delegate, _)), Some((ticket, reply))) =
        (&child_loop, child_loop_retained)
    {
        drive_until(servo, deadline, || popup_delegate.crashed.get())?;
        ensure!(
            popup_delegate.crash_reasons.borrow().as_slice() == [reason]
                && popup.pending_evaluations_for_document_scope_probe() == 0,
            "Unexpected popup failure notifications: {:?}",
            popup_delegate.crash_reasons.borrow()
        );
        popup_delegate.reject(ticket, reply)?;
    }
    // The popup in the parent's event loop keeps its scope and its retained reply.
    ensure!(
        parent_loop_delegate.active.get() == Some(parent_loop_document)
            && parent_loop_delegate.generation.get() == parent_loop_ticket.generation
            && !parent_loop_delegate.crashed.get(),
        "The failure revoked a document in the parent's event loop"
    );
    ensure!(
        parent_loop_delegate.deliver(parent_loop_ticket, parent_loop_reply),
        "The parent loop's retained reply lost its scope"
    );
    // Only the failed frame shows an error state. The crash page replaces the old child.
    let mut crashed_frame = None;
    drive_until(servo, deadline, || {
        crashed_frame = frame_state(&view)
            .ok()
            .filter(|state| state.0 != before.0 && state.1 && !state.3);
        crashed_frame.is_some()
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "The failed frame did not activate a new pipeline: {error}; frame={:?}",
            frame_state(&view).ok()
        )
    })?;
    let crashed_frame = crashed_frame.unwrap();
    ensure!(
        crashed_frame
            .2
            .as_deref()
            .is_some_and(|crash| crash.starts_with(reason)),
        "The failed frame did not get a crash page: {crashed_frame:?}"
    );
    // The crash page has an opaque origin, so the parent can no longer read the frame.
    let frame_replaced = "1:SecurityError";
    wait_text(servo, &view, deadline, parent, child_marker, frame_replaced)?;
    // The parent keeps its document, its scope, and its retained reply.
    ensure!(
        readiness(&delegate, parent) == ["dom-ready", "load-complete"]
            && delegate.active.get() == Some(parent)
            && delegate.generation.get() == ticket.generation
            && !delegate.crashed.get()
            && view.pending_evaluations_for_document_scope_probe() == 0,
        "The parent did not keep its document: readiness={:?}, active={:?}, crashes={:?}",
        readiness(&delegate, parent),
        delegate.active.get(),
        delegate.crash_reasons.borrow()
    );
    ensure!(
        delegate.deliver(ticket, reply),
        "The parent's retained reply lost its scope"
    );
    if let Some((parent_process, pending_process)) = processes {
        let crash_process = view.frame_content_process_id_for_document_scope_probe();
        ensure!(
            crash_process != parent_process
                && crash_process != pending_process
                && crash_process != std::process::id(),
            "The frame's crash page did not run in a new content process"
        );
    }
    let popup_crash = match &child_loop {
        Some((popup, popup_delegate, document)) => {
            drive_until(servo, deadline, || {
                popup_delegate
                    .active
                    .get()
                    .is_some_and(|id| id != *document)
            })?;
            let crash = identity(servo, popup, deadline)?;
            crash_page(servo, popup, deadline, crash, reason)?;
            Some(crash)
        }
        None => None,
    };
    // A late navigation response for the failed pipeline must not reach any live document.
    let errors_before_release = engine_errors.0.borrow().len();
    release_held_document(&delegate, PENDING_FRAME_NAVIGATION)?;
    settle(servo, deadline, 300)?;
    ensure!(
        identity(servo, &view, deadline)? == parent
            && frame_state(&view)? == crashed_frame
            && readiness(&delegate, parent) == ["dom-ready", "load-complete"],
        "The late navigation response changed the parent or the frame's crash page"
    );
    text(
        scoped(
            servo,
            &view,
            deadline,
            parent,
            &format!("String(window.frameState) + '|' + {child_marker}"),
        )?,
        parent,
        &format!("undefined|{frame_replaced}"),
    )?;
    if let (Some((popup, ..)), Some(popup_crash)) = (&child_loop, popup_crash) {
        ensure!(
            identity(servo, popup, deadline)? == popup_crash,
            "The late navigation response replaced the popup's crash document"
        );
        crash_page(servo, popup, deadline, popup_crash, reason)?;
    }
    ensure!(
        delegate.crash_reasons.borrow().is_empty()
            && child_loop
                .as_ref()
                .is_none_or(
                    |(_, popup_delegate, _)| popup_delegate.crash_reasons.borrow().len() == 1
                )
            && !parent_loop_delegate.crashed.get(),
        "The late navigation response caused another failure"
    );
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "The late navigation response raised engine errors: {late_errors:?}"
    );
    // Reload replaces the parent, and its frame loads its original child document again.
    view.reload();
    drive_until(servo, deadline, || {
        delegate.active.get().is_some_and(|id| id != parent)
    })?;
    wait_marker(servo, &view, deadline, parent_path)?;
    let reloaded = identity(servo, &view, deadline)?;
    ensure!(reloaded != parent, "Reload kept the parent scope");
    stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
    text(
        scoped(servo, &view, deadline, reloaded, "String(window.origin)")?,
        reloaded,
        ORIGIN,
    )?;
    drive_until(servo, deadline, || {
        readiness(&delegate, reloaded).contains(&"load-complete")
    })?;
    text(
        scoped(servo, &view, deadline, reloaded, child_marker)?,
        reloaded,
        &child_loaded,
    )?;
    load(servo, &view, deadline, "/after-pending-frame-failure")?;
    let recovered = identity(servo, &view, deadline)?;
    text(
        scoped(
            servo,
            &view,
            deadline,
            recovered,
            "'pending-frame-recovered'",
        )?,
        recovered,
        "pending-frame-recovered",
    )?;
    if let (Some((popup, _, document)), Some(popup_crash)) = (&child_loop, popup_crash) {
        stale(scoped(servo, popup, deadline, *document, "'obsolete'")?)?;
        load(servo, popup, deadline, "/after-pending-frame-popup")?;
        let popup_recovered = identity(servo, popup, deadline)?;
        ensure!(
            popup_recovered != popup_crash && popup_recovered != *document,
            "The revoked popup did not navigate"
        );
        text(
            scoped(servo, popup, deadline, popup_recovered, "'popup-recovered'")?,
            popup_recovered,
            "popup-recovered",
        )?;
    }
    text(
        scoped(
            servo,
            &parent_loop,
            deadline,
            parent_loop_document,
            "'parent-loop-survivor'",
        )?,
        parent_loop_document,
        "parent-loop-survivor",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'pending-frame-survivor'",
        )?,
        survivor_document,
        "pending-frame-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8
            && popup_audit
                .as_ref()
                .is_none_or(|audit| audit.calls.get() == 8),
        "Pending frame recovery repeated callbacks"
    );
    if let Some(server) = server {
        let served = server.finish()?;
        ensure!(
            served.paths == [FRAME_SIBLING],
            "Unexpected loopback requests: {:?}",
            served.paths
        );
    }
    Ok(format!(
        "{}, pending child pipeline {}, old child stayed active, parent kept its document, scope, and retained reply, 8 held parent callbacks completed, frame crash page replaced the old child, {}, late navigation response ignored, reload and new URL recover, survivor usable",
        failure.how(),
        match site {
            FrameSite::CrossSite => "in the child-site popup's event loop",
            _ => "alone in a new event loop",
        },
        match site {
            FrameSite::CrossSite => {
                "child-site popup revoked before 8 callbacks and recovered, parent-loop popup kept scope"
            }
            _ => "parent-loop popup kept scope",
        }
    ))
}

// Frame containment paths beyond a parsing child and a loaded child's pending navigation.
// Each parent counts its frame's load events. A cross-site child reports its path by message.
const FRAME_LOADS: &str = "onload='window.frameLoads = (window.frameLoads || 0) + 1'";
const INITIAL_FRAME: &str = "https://initial-frame.invalid/held-initial-frame";
const HEALTHY_PENDING: &str = "https://healthy-pending.invalid/held-healthy-navigation";
const FRAME_ACTIVE_CHILD: &str = "/frame-active-child";
const FRAME_HISTORY_FIRST: &str = "/frame-history-first";
const FRAME_HISTORY_SECOND: &str = "/frame-history-second";
// The parent's view of its frame: the last reported path, the frame's load events, the joint
// session history length, and the frame's URL or the error that a cross-origin read raises.
const FRAME_VIEW: &str = "String(window.frameState) + '|' + String(window.frameLoads) + '|' + history.length + '|' + (() => { try { return String(frames[0].location.href); } catch (error) { return error.name; } })()";

#[derive(Clone, Copy, PartialEq)]
enum FramePath {
    // A newly inserted frame's first navigation waits for its response headers alone in a new
    // event loop. The frame's current document is the initial `about:blank` in the parent's loop.
    Initial,
    // A loaded cross-site child fails while the frame's next navigation waits for its response
    // headers in a third event loop.
    HealthyPending,
    // A loaded cross-site child fails while an earlier document of the frame is in history.
    History,
}

impl FramePath {
    fn parent_path(self) -> &'static str {
        match self {
            Self::Initial => "/frame-initial-parent",
            Self::HealthyPending => "/frame-healthy-pending-parent",
            Self::History => "/frame-history-parent",
        }
    }

    fn frame_src(self) -> String {
        match self {
            Self::Initial => INITIAL_FRAME.to_owned(),
            Self::HealthyPending => format!("{FRAME_ORIGIN}{FRAME_ACTIVE_CHILD}"),
            Self::History => format!("{FRAME_ORIGIN}{FRAME_HISTORY_FIRST}"),
        }
    }

    // The parent's view of its frame immediately before the failure.
    fn before(self) -> String {
        match self {
            Self::Initial => "undefined|undefined|1|about:blank".to_owned(),
            Self::HealthyPending => format!("{FRAME_ACTIVE_CHILD}|1|1|SecurityError"),
            Self::History => format!("{FRAME_HISTORY_SECOND}|2|2|SecurityError"),
        }
    }

    // The parent's view of its frame after the error page loads.
    fn after(self) -> String {
        match self {
            Self::Initial => "undefined|1|1|SecurityError".to_owned(),
            Self::HealthyPending => format!("{FRAME_ACTIVE_CHILD}|2|1|SecurityError"),
            Self::History => format!("{FRAME_HISTORY_SECOND}|3|2|SecurityError"),
        }
    }
}

// Fail a child frame in another event loop than its parent's, and check the contained result.
#[allow(clippy::too_many_arguments)]
fn frame_path_failure(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    multiprocess: bool,
    survivor: (&WebView, DocumentIdentity),
    path: FramePath,
    failure: ParseFailure,
) -> Result<String> {
    let reason = failure.reason();
    let parent_path = path.parent_path();
    let delegate = Rc::new(Delegate::default());
    *delegate.frame_markup.borrow_mut() = Some((
        parent_path.into(),
        format!(
            "{FRAME_LISTENER}<iframe {FRAME_LOADS} src='{}'></iframe>",
            path.frame_src()
        ),
    ));
    let view = WebViewBuilder::new(servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}{parent_path}"))?)
        .delegate(delegate.clone())
        .build();
    drive_until(servo, deadline, || delegate.active.get().is_some())?;
    wait_marker(servo, &view, deadline, parent_path)?;
    let parent = identity(servo, &view, deadline)?;
    let mut first = None;
    match path {
        FramePath::Initial => {
            // The parent's load event waits for the frame's first navigation.
            drive_until(servo, deadline, || {
                held_count(&delegate, "/held-initial-frame") == 1
                    && readiness(&delegate, parent).contains(&"dom-ready")
            })?;
        }
        FramePath::HealthyPending => {
            wait_text(servo, &view, deadline, parent, FRAME_VIEW, &path.before())?;
            drive_until(servo, deadline, || {
                readiness(&delegate, parent).contains(&"load-complete")
            })?;
            text(
                scoped(
                    servo,
                    &view,
                    deadline,
                    parent,
                    &format!(
                        "document.querySelector('iframe').src = {HEALTHY_PENDING:?}; 'navigating'"
                    ),
                )?,
                parent,
                "navigating",
            )?;
            drive_until(servo, deadline, || {
                held_count(&delegate, "/held-healthy-navigation") == 1
            })?;
        }
        FramePath::History => {
            wait_text(
                servo,
                &view,
                deadline,
                parent,
                FRAME_VIEW,
                &format!("{FRAME_HISTORY_FIRST}|1|1|SecurityError"),
            )?;
            drive_until(servo, deadline, || {
                readiness(&delegate, parent).contains(&"load-complete")
            })?;
            first = Some(frame_state(&view)?.0);
            text(
                scoped(
                    servo,
                    &view,
                    deadline,
                    parent,
                    &format!(
                        "document.querySelector('iframe').src = '{FRAME_ORIGIN}{FRAME_HISTORY_SECOND}'; 'navigating'"
                    ),
                )?,
                parent,
                "navigating",
            )?;
        }
    }
    wait_text(servo, &view, deadline, parent, FRAME_VIEW, &path.before())?;
    let parent_readiness: &[&str] = match path {
        FramePath::Initial => &["dom-ready"],
        _ => &["dom-ready", "load-complete"],
    };
    ensure!(
        delegate.active.get() == Some(parent) && readiness(&delegate, parent) == parent_readiness,
        "Unexpected parent lifecycle before the frame failure: {:?}",
        readiness(&delegate, parent)
    );
    ensure!(
        view.crash_pending_script_thread_for_document_scope_probe()
            .is_none(),
        "The frame navigation created a pending top-level pipeline"
    );
    // This popup's `about:blank` document shares the parent's event loop.
    let (parent_loop, parent_loop_delegate, parent_loop_document) =
        open_popup(servo, &view, &delegate, context.clone(), deadline, parent)?;
    let processes = if multiprocess {
        let host = std::process::id();
        let parent_process = view.content_process_id_for_document_scope_probe();
        let frame_process = view.frame_content_process_id_for_document_scope_probe();
        let failed_process = if path == FramePath::Initial {
            ensure!(
                frame_process == parent_process,
                "The initial about:blank document did not share the parent's process"
            );
            view.pending_frame_content_process_id_for_document_scope_probe()
        } else {
            frame_process
        };
        ensure!(
            parent_process != host
                && parent_loop.content_process_id_for_document_scope_probe() == parent_process
                && failed_process != host
                && failed_process != parent_process
                && survivor.0.content_process_id_for_document_scope_probe() != failed_process,
            "The failed frame pipeline did not run in a process of its own"
        );
        if path == FramePath::HealthyPending {
            let pending_process = view.pending_frame_content_process_id_for_document_scope_probe();
            ensure!(
                pending_process != host
                    && pending_process != parent_process
                    && pending_process != failed_process,
                "The healthy pending navigation did not run in a third process"
            );
        }
        Some((parent_process, failed_process))
    } else {
        None
    };
    // The constellation's record of the frame. Only the initial and healthy pending paths have a
    // pending pipeline.
    let before = frame_state(&view)?;
    ensure!(
        before.1 && before.2.is_none() && before.3 == (path != FramePath::History),
        "Unexpected child frame state before the failure: {before:?}"
    );
    let ticket = delegate.ticket(parent)?;
    let reply = scoped(servo, &view, deadline, parent, "'before-frame-failure'")?;
    ensure!(reply.is_ok(), "Frame path control failed");
    let parent_loop_ticket = parent_loop_delegate.ticket(parent_loop_document)?;
    let parent_loop_reply = scoped(
        servo,
        &parent_loop,
        deadline,
        parent_loop_document,
        "'parent-loop-retained'",
    )?;
    ensure!(
        parent_loop_reply.is_ok(),
        "Parent-loop popup control failed"
    );
    // In the initial path the failed pipeline is pending, so the frame's current document stays.
    // Otherwise the frame's current pipeline is the failed one. A healthy pending pipeline closes.
    let gone = |state: &(_, bool, Option<String>, bool)| {
        state.0 == before.0 && state.1 == (path == FramePath::Initial) && !state.3
    };
    let mut separate = None;
    // Hold the parent's script thread, so its callbacks cannot complete, and the frame cannot
    // navigate, until the constellation has handled the failure.
    let (audit, handled, held) = view.with_paused_script_for_document_scope_probe(|| {
        let audit = CleanupAudit::queue(&view, Some(parent), &delegate, true);
        match (path, failure) {
            (FramePath::Initial, ParseFailure::ScriptThread) => {
                separate = view.crash_pending_frame_script_thread_for_document_scope_probe();
            }
            (FramePath::Initial, ParseFailure::ProcessExit) => {
                view.with_terminating_pending_frame_content_process_for_document_scope_probe(|| ())
            }
            (FramePath::Initial, ParseFailure::ProcessKill) => {
                view.with_killed_pending_frame_content_process_for_document_scope_probe(|| ())
            }
            (_, ParseFailure::ScriptThread) => {
                separate = view.crash_frame_script_thread_for_document_scope_probe();
            }
            (_, ParseFailure::ProcessExit) => {
                view.with_terminating_frame_content_process_for_document_scope_probe(|| ())
            }
            (_, ParseFailure::ProcessKill) => {
                view.with_killed_frame_content_process_for_document_scope_probe(|| ())
            }
        }
        let barrier = deadline.min(Instant::now() + Duration::from_secs(4));
        // A revocation of the parent also ends this wait, so the check below reports it.
        let handled = drive_until(servo, barrier, || {
            frame_state(&view).is_ok_and(|state| gone(&state))
                || audit.calls.get() > 0
                || delegate.crashed.get()
        })
        .and_then(|()| settle(servo, barrier, 200));
        let held = (
            audit.calls.get(),
            delegate.active.get(),
            delegate.crash_reasons.borrow().clone(),
            frame_state(&view).ok(),
        );
        (audit, handled, held)
    });
    ensure!(
        failure != ParseFailure::ScriptThread || separate == Some(true),
        "Probe did not reach a frame pipeline in a separate event loop: {separate:?}"
    );
    handled.map_err(|error| {
        anyhow::anyhow!(
            "The frame {} was not handled within the barrier: {error}; frame={:?}",
            failure.how(),
            frame_state(&view).ok(),
        )
    })?;
    let (calls, active, crashes, frame_held) = held;
    ensure!(
        (calls, active, crashes.as_slice()) == (0, Some(parent), &[][..]),
        "The frame {} revoked the healthy parent: calls={calls}, active={active:?}, crashes={crashes:?}",
        failure.how()
    );
    ensure!(
        frame_held.as_ref().is_some_and(gone),
        "The failed frame pipeline was not closed: {frame_held:?}"
    );
    // The parent's held callbacks then complete in the same document.
    drive_until(servo, deadline, || audit.calls.get() == 8)?;
    audit.check_completed(parent)?;
    ensure!(
        parent_loop_delegate.active.get() == Some(parent_loop_document)
            && parent_loop_delegate.generation.get() == parent_loop_ticket.generation
            && !parent_loop_delegate.crashed.get(),
        "The failure revoked a document in the parent's event loop"
    );
    ensure!(
        parent_loop_delegate.deliver(parent_loop_ticket, parent_loop_reply),
        "The parent loop's retained reply lost its scope"
    );
    // Only the failed frame shows an error state.
    let mut crashed_frame = None;
    drive_until(servo, deadline, || {
        crashed_frame = frame_state(&view)
            .ok()
            .filter(|state| state.0 != before.0 && state.1 && !state.3);
        crashed_frame.is_some()
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "The failed frame did not activate a new pipeline: {error}; frame={:?}",
            frame_state(&view).ok()
        )
    })?;
    let crashed_frame = crashed_frame.unwrap();
    ensure!(
        crashed_frame
            .2
            .as_deref()
            .is_some_and(|crash| crash.starts_with(reason))
            && first.is_none_or(|first| crashed_frame.0 != first),
        "The failed frame did not get a crash page: {crashed_frame:?}"
    );
    // The parent's load event no longer waits for the frame's first navigation.
    drive_until(servo, deadline, || {
        readiness(&delegate, parent).contains(&"load-complete")
    })
    .map_err(|error| {
        anyhow::anyhow!(
            "The parent's load did not complete after the frame's crash page: {error}; readiness={:?}",
            readiness(&delegate, parent)
        )
    })?;
    wait_text(servo, &view, deadline, parent, FRAME_VIEW, &path.after())?;
    ensure!(
        readiness(&delegate, parent) == ["dom-ready", "load-complete"]
            && delegate.active.get() == Some(parent)
            && delegate.generation.get() == ticket.generation
            && !delegate.crashed.get()
            && view.pending_evaluations_for_document_scope_probe() == 0,
        "The parent did not keep its document: readiness={:?}, active={:?}, crashes={:?}",
        readiness(&delegate, parent),
        delegate.active.get(),
        delegate.crash_reasons.borrow()
    );
    ensure!(
        delegate.deliver(ticket, reply),
        "The parent's retained reply lost its scope"
    );
    if let Some((parent_process, failed_process)) = processes {
        let crash_process = view.frame_content_process_id_for_document_scope_probe();
        ensure!(
            crash_process != parent_process
                && crash_process != failed_process
                && crash_process != std::process::id(),
            "The frame's crash page did not run in a new content process"
        );
    }
    // A late navigation response must not reach any live document.
    let errors_before_release = engine_errors.0.borrow().len();
    match path {
        FramePath::Initial => release_held_document(&delegate, "/held-initial-frame")?,
        FramePath::HealthyPending => release_held_document(&delegate, "/held-healthy-navigation")?,
        FramePath::History => ensure!(
            delegate.held.borrow().is_empty(),
            "Unexpected withheld request"
        ),
    }
    settle(servo, deadline, 300)?;
    ensure!(
        identity(servo, &view, deadline)? == parent
            && frame_state(&view)? == crashed_frame
            && readiness(&delegate, parent) == ["dom-ready", "load-complete"]
            && delegate.crash_reasons.borrow().is_empty()
            && !parent_loop_delegate.crashed.get(),
        "The late navigation response changed the parent or the frame's crash page: frame={:?}",
        frame_state(&view).ok()
    );
    text(
        scoped(servo, &view, deadline, parent, FRAME_VIEW)?,
        parent,
        &path.after(),
    )?;
    let late_errors = engine_errors.0.borrow()[errors_before_release..].to_vec();
    ensure!(
        late_errors.is_empty(),
        "The late navigation response raised engine errors: {late_errors:?}"
    );
    let detail = match path {
        FramePath::Initial => {
            // Reload replaces the parent. Its frame's first navigation waits again.
            view.reload();
            drive_until(servo, deadline, || {
                delegate.active.get().is_some_and(|id| id != parent)
            })?;
            wait_marker(servo, &view, deadline, parent_path)?;
            let reloaded = identity(servo, &view, deadline)?;
            ensure!(reloaded != parent, "Reload kept the parent scope");
            stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
            drive_until(servo, deadline, || {
                held_count(&delegate, "/held-initial-frame") == 1
            })?;
            text(
                scoped(servo, &view, deadline, reloaded, FRAME_VIEW)?,
                reloaded,
                &path.before(),
            )?;
            load(servo, &view, deadline, "/after-frame-initial-failure")?;
            ensure!(
                cancel_held(&delegate) == 1,
                "Expected one withheld navigation"
            );
            "frame's first navigation pending alone in a new event loop, parent's load event waited, crash page replaced the initial about:blank, frame load event fired once, parent's load completed, late response ignored, reload waits again, new URL recovers"
        }
        FramePath::HealthyPending => {
            view.reload();
            drive_until(servo, deadline, || {
                delegate.active.get().is_some_and(|id| id != parent)
            })?;
            wait_marker(servo, &view, deadline, parent_path)?;
            let reloaded = identity(servo, &view, deadline)?;
            ensure!(reloaded != parent, "Reload kept the parent scope");
            stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
            wait_text(
                servo,
                &view,
                deadline,
                reloaded,
                FRAME_VIEW,
                &format!("{FRAME_ACTIVE_CHILD}|1|1|SecurityError"),
            )?;
            load(
                servo,
                &view,
                deadline,
                "/after-frame-healthy-pending-failure",
            )?;
            "loaded child failed with a healthy pending navigation in a third event loop, pending navigation closed, crash page replaced the child, frame load event fired, late response ignored, reload and new URL recover"
        }
        FramePath::History => {
            let crash = crashed_frame.0;
            // Back within the frame rebuilds the first child in a new pipeline.
            let traversal = view.go_back(1);
            drive_until(servo, deadline, || {
                delegate.traversals.borrow().contains(&traversal)
            })?;
            let back_view = format!("{FRAME_HISTORY_FIRST}|3|2|SecurityError");
            wait_text(servo, &view, deadline, parent, FRAME_VIEW, &back_view)?;
            let back = frame_state(&view)?;
            ensure!(
                back.0 != crash && Some(back.0) != first && back.1 && back.2.is_none() && !back.3,
                "Back did not rebuild the first child: {back:?}"
            );
            if let Some((_, failed_process)) = processes {
                ensure!(
                    view.frame_content_process_id_for_document_scope_probe() != failed_process,
                    "The rebuilt child ran in the failed process"
                );
            }
            // Forward within the frame restores the same error page.
            let traversal = view.go_forward(1);
            drive_until(servo, deadline, || {
                delegate.traversals.borrow().contains(&traversal)
            })?;
            let mut forward = None;
            drive_until(servo, deadline, || {
                forward = frame_state(&view).ok().filter(|state| state.0 != back.0);
                forward.is_some()
            })?;
            ensure!(
                forward.as_ref() == Some(&crashed_frame),
                "Forward did not restore the frame's crash page: {forward:?}"
            );
            text(
                scoped(servo, &view, deadline, parent, FRAME_VIEW)?,
                parent,
                &back_view,
            )?;
            ensure!(
                delegate.active.get() == Some(parent)
                    && delegate.generation.get() == ticket.generation
                    && !delegate.crashed.get(),
                "Frame traversal changed the parent's scope"
            );
            // A top-level navigation and back restore the parent with its frame's crash page.
            load(servo, &view, deadline, "/after-frame-history-failure")?;
            let next = identity(servo, &view, deadline)?;
            let traversal = view.go_back(1);
            drive_until(servo, deadline, || {
                delegate.traversals.borrow().contains(&traversal)
            })?;
            wait_marker(servo, &view, deadline, parent_path)?;
            let restored = identity(servo, &view, deadline)?;
            ensure!(
                restored != parent && restored != next,
                "Back to the parent kept an old scope"
            );
            stale(scoped(servo, &view, deadline, parent, "'obsolete'")?)?;
            ensure!(
                frame_state(&view)? == crashed_frame,
                "Back to the parent did not keep the frame's crash page: {:?}",
                frame_state(&view).ok()
            );
            text(
                scoped(servo, &view, deadline, restored, FRAME_VIEW)?,
                restored,
                &format!("{FRAME_HISTORY_FIRST}|3|3|SecurityError"),
            )?;
            "crash page replaced the failed child's history entry, back rebuilt the first child in a new pipeline, forward restored the same crash page, top-level back restored the parent with the crash page"
        }
    };
    text(
        scoped(
            servo,
            &parent_loop,
            deadline,
            parent_loop_document,
            "'parent-loop-survivor'",
        )?,
        parent_loop_document,
        "parent-loop-survivor",
    )?;
    let (survivor, survivor_document) = survivor;
    text(
        scoped(
            servo,
            survivor,
            deadline,
            survivor_document,
            "'frame-path-survivor'",
        )?,
        survivor_document,
        "frame-path-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "Frame path recovery repeated callbacks"
    );
    Ok(format!(
        "{}, parent kept its document, scope, and retained reply, 8 held parent callbacks completed, parent-loop popup kept scope, {detail}, survivor usable",
        failure.how()
    ))
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
    // The host's active document at delivery, and the callback's outcome.
    replies: Receiver<(Option<DocumentIdentity>, CallbackOutcome)>,
}

// The scoped result's document, `None` for an unscoped result, or the error.
type CallbackOutcome = Result<Option<DocumentIdentity>, JavaScriptEvaluationError>;

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
            let callback = move |outcome| {
                drop(capture);
                drop(retained_view);
                calls.set(calls.get() + 1);
                let _ = sender.send((delegate.active.get(), outcome));
                if panic_after_delivery {
                    panic!("{CALLBACK_PANIC}");
                }
            };
            if let Some(document) = document.filter(|_| index % 2 == 0) {
                view.evaluate_javascript_in_document(document, "'pending'", move |result| {
                    callback(result.map(|result| Some(result.document)));
                });
            } else {
                view.evaluate_javascript("'pending'", move |result| {
                    callback(result.map(|_| None));
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
            let (active, outcome) = self.replies.try_recv()?;
            ensure!(
                active.is_none()
                    && matches!(outcome, Err(JavaScriptEvaluationError::StaleDocument)),
                "Cleanup callback preceded revocation or returned success"
            );
        }
        self.check_released()
    }

    // The document kept its scope. Every callback succeeds in it, and each scoped result
    // carries its identity.
    fn check_completed(&self, document: DocumentIdentity) -> Result<()> {
        ensure!(self.calls.get() == 8, "Expected eight terminal callbacks");
        let mut scoped = 0;
        for _ in 0..8 {
            let (active, outcome) = self.replies.try_recv()?;
            ensure!(
                active == Some(document)
                    && outcome
                        .as_ref()
                        .is_ok_and(|result| result.is_none_or(|id| id == document)),
                "A callback of a surviving document failed: active={active:?}, outcome={outcome:?}"
            );
            scoped += usize::from(matches!(outcome, Ok(Some(_))));
        }
        ensure!(scoped == 4, "Expected four scoped results");
        self.check_released()
    }

    fn check_released(&self) -> Result<()> {
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

pub(super) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic");
        log::error!("{message}");
    }));
}

fn start(multiprocess: bool) -> Result<(Rc<SoftwareRenderingContext>, Servo)> {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
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
            multiprocess,
            ..Opts::default()
        })
        .event_loop_waker(Box::new(Waker(thread::current())))
        .build();
    // Servo's own crash tests use this public logging path to report thread panics.
    servo.setup_logging();
    install_panic_hook();
    Ok((context, servo))
}

// The documents part: lifecycle, host gate, cancellation, interruption, unwinds of top-level
// documents, release, callback and delegate panics, and explicit shutdown.
fn run_documents(multiprocess: bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(if multiprocess { 80 } else { 30 });
    let (context, servo) = start(multiprocess)?;
    // Engine errors are observations only. A case asserts on them only where it says so.
    let engine_errors = Rc::new(BackendErrors::default());
    servo.set_delegate(engine_errors.clone());
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
    crash_page(
        &servo,
        &failure,
        deadline,
        crash_document,
        "S1 probe actual script-thread panic",
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

    // Withhold the first navigation response. The view then has a pending pipeline, but
    // no document and no top browsing context in the constellation.
    let unbuilt_delegate = Rc::new(Delegate::default());
    let unbuilt = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/held-initial"))?)
        .delegate(unbuilt_delegate.clone())
        .build();
    drive_until(&servo, deadline, || {
        !unbuilt_delegate.held.borrow().is_empty()
    })?;
    ensure!(
        unbuilt_delegate.active.get().is_none() && unbuilt_delegate.ticket(recovered).is_err(),
        "Host admitted work before document construction"
    );
    // Every request form must fail at the constellation without a pending callback.
    // A foreign identity must not select another view's document.
    let early = Rc::new(RefCell::new(Vec::new()));
    let capture = Rc::new(());
    let early_released = Rc::downgrade(&capture);
    for index in 0..9 {
        let early = early.clone();
        let capture = capture.clone();
        let finish = move |error: Option<JavaScriptEvaluationError>| {
            drop(capture);
            early.borrow_mut().push(error);
        };
        match index % 3 {
            0 => unbuilt.request_document_identity(move |result| finish(result.err())),
            1 => unbuilt
                .evaluate_javascript("'must-not-execute'", move |result| finish(result.err())),
            _ => unbuilt.evaluate_javascript_in_document(
                recovered,
                "'must-not-execute'",
                move |result| finish(result.err()),
            ),
        }
    }
    drop(capture);
    drive_until(&servo, deadline, || early.borrow().len() == 9)?;
    ensure!(
        early
            .borrow()
            .iter()
            .all(|error| matches!(error, Some(JavaScriptEvaluationError::InternalError))),
        "Evaluation without a top browsing context did not fail: {:?}",
        early.borrow()
    );
    ensure!(
        early_released.upgrade().is_none()
            && unbuilt.pending_evaluations_for_document_scope_probe() == 0,
        "Evaluation without a top browsing context retained callbacks"
    );
    ensure!(
        unbuilt.crash_pending_script_thread_for_document_scope_probe() == Some(true),
        "Probe did not reach a pending pipeline without an active document"
    );
    // The panic handler revokes before it finds no top browsing context and returns early.
    // Pipeline exit then reports that no document is active. In multiprocess mode, the
    // second event can follow the crash notification. A request sent between them would
    // be revoked by it, so wait for both before the next request.
    drive_until(&servo, deadline, || {
        unbuilt_delegate.crashed.get() && unbuilt_delegate.events.borrow().len() >= 2
    })?;
    ensure!(
        unbuilt_delegate.crash_reasons.borrow().as_slice()
            == ["S1 probe actual script-thread panic"],
        "Unconstructed view missed the unwind notification: {:?}",
        unbuilt_delegate.crash_reasons.borrow()
    );
    // No crash document can follow, so the view never activates a document.
    ensure!(
        unbuilt_delegate.active.get().is_none()
            && unbuilt_delegate.generation.get() == 0
            && unbuilt_delegate.loads.get() == 0
            && unbuilt_delegate.events.borrow().as_slice() == [DocumentLifecycle::Active(None); 2],
        "Unconstructed view received a document: {:?}",
        unbuilt_delegate.events.borrow()
    );
    ensure!(
        cancel_held(&unbuilt_delegate) == 1,
        "Expected one withheld navigation"
    );
    let after = Rc::new(RefCell::new(None));
    let reply = after.clone();
    unbuilt.request_document_identity(move |result| *reply.borrow_mut() = Some(result));
    drive_until(&servo, deadline, || after.borrow().is_some())?;
    let after = after.take();
    ensure!(
        matches!(after, Some(Err(JavaScriptEvaluationError::InternalError))),
        "Failed unconstructed view accepted evaluation: {after:?}"
    );
    ensure!(
        unbuilt.pending_evaluations_for_document_scope_probe() == 0,
        "Failed unconstructed view retained callbacks"
    );
    // The view cannot navigate without a top browsing context. Close and release it.
    unbuilt.close_for_document_scope_probe();
    drive_until(&servo, deadline, || unbuilt_delegate.closed.get())?;
    drop(unbuilt);
    ensure!(
        unbuilt_delegate.released.get(),
        "Final release of the failed view did not notify the host"
    );
    text(
        scoped(&servo, &failure, deadline, recovered, "'unbuilt-survivor'")?,
        recovered,
        "unbuilt-survivor",
    )?;
    println!(
        "S1 unconstructed-view-failure: PASS (9 early requests failed without retention, pending pipeline unwound, no crash document)"
    );

    // A cross-site navigation uses another event loop. Withhold its response, then unwind
    // that loop while eight callbacks for the active document wait at a script barrier.
    let pending_delegate = Rc::new(Delegate::default());
    let pending_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/navigation-first"))?)
        .delegate(pending_delegate.clone())
        .build();
    drive_until(&servo, deadline, || pending_delegate.loads.get() > 0)?;
    let first = identity(&servo, &pending_view, deadline)?;
    load(&servo, &pending_view, deadline, "/navigation-source")?;
    let source = identity(&servo, &pending_view, deadline)?;
    let source_ticket = pending_delegate.ticket(source)?;
    let source_reply = scoped(
        &servo,
        &pending_view,
        deadline,
        source,
        "'before-pending-failure'",
    )?;
    ensure!(source_reply.is_ok(), "Pending navigation control failed");
    pending_view.load(url::Url::parse("https://pending.invalid/held-navigation")?);
    drive_until(&servo, deadline, || {
        !pending_delegate.held.borrow().is_empty()
    })?;
    ensure!(
        pending_delegate.active.get() == Some(source),
        "A pending navigation revoked the active document before failure"
    );
    let mut separate = None;
    let (audit, revoked) = pending_view.with_paused_script_for_document_scope_probe(|| {
        let audit = CleanupAudit::queue(&pending_view, Some(source), &pending_delegate, true);
        separate = pending_view.crash_pending_script_thread_for_document_scope_probe();
        // The active document's script thread stays healthy. Wait inside the barrier,
        // so only revocation can complete these callbacks.
        let revoked = drive_until(
            &servo,
            deadline.min(Instant::now() + Duration::from_secs(4)),
            || audit.calls.get() == 8,
        );
        (audit, revoked)
    });
    ensure!(
        separate == Some(true),
        "Probe did not reach a pending pipeline in a separate event loop: {separate:?}"
    );
    revoked.map_err(|error| {
        anyhow::anyhow!(
            "Pending pipeline failure did not revoke within the barrier: {error}; active={:?}, calls={}",
            pending_delegate.active.get(),
            audit.calls.get()
        )
    })?;
    audit.check()?;
    drive_until(&servo, deadline, || pending_delegate.crashed.get())?;
    ensure!(
        pending_delegate.crash_reasons.borrow().as_slice()
            == ["S1 probe actual script-thread panic"],
        "Pending pipeline failure missed the unwind notification: {:?}",
        pending_delegate.crash_reasons.borrow()
    );
    ensure!(
        pending_view.pending_evaluations_for_document_scope_probe() == 0,
        "Pending pipeline failure retained callbacks"
    );
    pending_delegate.reject(source_ticket, source_reply)?;
    ensure!(
        cancel_held(&pending_delegate) == 1,
        "Expected one withheld navigation"
    );
    // Upstream recovery replaces the healthy active document with a crash document.
    drive_until(&servo, deadline, || {
        pending_delegate.active.get().is_some_and(|id| id != source)
    })?;
    let pending_crash = identity(&servo, &pending_view, deadline)?;
    crash_page(
        &servo,
        &pending_view,
        deadline,
        pending_crash,
        "S1 probe actual script-thread panic",
    )?;
    // An error page must recover through reload, back, forward, and a new URL.
    // Recovery closes every history pipeline in the view, so no entry may keep one.
    // Reload of the crash page must load the failed view's previous URL again.
    pending_view.reload();
    drive_until(&servo, deadline, || {
        pending_delegate
            .active
            .get()
            .is_some_and(|id| id != pending_crash)
    })?;
    wait_marker(&servo, &pending_view, deadline, "/navigation-source")?;
    let reloaded = identity(&servo, &pending_view, deadline)?;
    // The crash page has every sandboxing flag. The reloaded page must not inherit them.
    text(
        scoped(
            &servo,
            &pending_view,
            deadline,
            reloaded,
            "String(window.origin)",
        )?,
        reloaded,
        ORIGIN,
    )?;
    let traversal = pending_view.go_back(1);
    drive_until(&servo, deadline, || {
        pending_delegate.traversals.borrow().contains(&traversal)
    })?;
    wait_marker(&servo, &pending_view, deadline, "/navigation-first")?;
    let back = identity(&servo, &pending_view, deadline)?;
    ensure!(
        back != first,
        "Back after crash recovery revived a closed pipeline's scope"
    );
    stale(scoped(
        &servo,
        &pending_view,
        deadline,
        first,
        "'obsolete'",
    )?)?;
    let traversal = pending_view.go_forward(1);
    drive_until(&servo, deadline, || {
        pending_delegate.traversals.borrow().contains(&traversal)
    })?;
    wait_marker(&servo, &pending_view, deadline, "/navigation-source")?;
    let forward = identity(&servo, &pending_view, deadline)?;
    ensure!(
        forward != reloaded,
        "Forward after crash recovery revived the old scope"
    );
    text(
        scoped(&servo, &pending_view, deadline, forward, "'forward'")?,
        forward,
        "forward",
    )?;
    println!(
        "S1 crash-recovery-history: PASS (reload, back to a closed history entry, and forward recover)"
    );
    load(&servo, &pending_view, deadline, "/after-pending-failure")?;
    let pending_recovered = identity(&servo, &pending_view, deadline)?;
    ensure!(
        pending_recovered != source,
        "Pending failure recovery revived the old scope"
    );
    stale(scoped(
        &servo,
        &pending_view,
        deadline,
        source,
        "'obsolete'",
    )?)?;
    text(
        scoped(
            &servo,
            &pending_view,
            deadline,
            pending_recovered,
            "'pending-recovered'",
        )?,
        pending_recovered,
        "pending-recovered",
    )?;
    text(
        scoped(&servo, &failure, deadline, recovered, "'pending-survivor'")?,
        recovered,
        "pending-survivor",
    )?;
    ensure!(
        audit.calls.get() == 8,
        "Pending failure recovery repeated callbacks"
    );
    println!(
        "S1 pending-navigation-failure: PASS (separate event loop unwound before construction, 8 callbacks released, recovery)"
    );
    // A crash page can stay in history. A later failure closes every history pipeline, so
    // back rebuilds that crash page in a new pipeline. Reload from the rebuilt page must
    // still load the failed document without the crash page's sandboxing.
    let history_delegate = Rc::new(Delegate::default());
    let history_view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/crash-history-source"))?)
        .delegate(history_delegate.clone())
        .build();
    drive_until(&servo, deadline, || history_delegate.loads.get() > 0)?;
    let history_source = identity(&servo, &history_view, deadline)?;
    let probe_crashes = |count| {
        let reasons = history_delegate.crash_reasons.borrow();
        reasons.len() == count
            && reasons
                .iter()
                .all(|reason| reason == "S1 probe actual script-thread panic")
    };
    history_view.crash_script_thread_for_document_scope_probe();
    drive_until(&servo, deadline, || probe_crashes(1))?;
    drive_until(&servo, deadline, || {
        history_delegate
            .active
            .get()
            .is_some_and(|id| id != history_source)
    })?;
    let first_crash = identity(&servo, &history_view, deadline)?;
    crash_page(
        &servo,
        &history_view,
        deadline,
        first_crash,
        "S1 probe actual script-thread panic",
    )?;
    load(&servo, &history_view, deadline, "/crash-history-next")?;
    let history_next = identity(&servo, &history_view, deadline)?;
    history_view.crash_script_thread_for_document_scope_probe();
    drive_until(&servo, deadline, || probe_crashes(2))?;
    drive_until(&servo, deadline, || {
        history_delegate
            .active
            .get()
            .is_some_and(|id| id != history_next)
    })?;
    let second_crash = identity(&servo, &history_view, deadline)?;
    crash_page(
        &servo,
        &history_view,
        deadline,
        second_crash,
        "S1 probe actual script-thread panic",
    )?;
    let traversal = history_view.go_back(1);
    drive_until(&servo, deadline, || {
        history_delegate.traversals.borrow().contains(&traversal)
    })?;
    let rebuilt_crash = identity(&servo, &history_view, deadline)?;
    ensure!(
        rebuilt_crash != first_crash && rebuilt_crash != second_crash,
        "Back did not rebuild the closed crash entry"
    );
    stale(scoped(
        &servo,
        &history_view,
        deadline,
        first_crash,
        "'obsolete'",
    )?)?;
    crash_page(
        &servo,
        &history_view,
        deadline,
        rebuilt_crash,
        "S1 probe actual script-thread panic",
    )?;
    history_view.reload();
    drive_until(&servo, deadline, || {
        history_delegate
            .active
            .get()
            .is_some_and(|id| id != rebuilt_crash)
    })?;
    let history_reloaded = identity(&servo, &history_view, deadline)?;
    // Check the origin first. A reload that kept the sandboxing never runs the marker script.
    text(
        scoped(
            &servo,
            &history_view,
            deadline,
            history_reloaded,
            "String(window.origin)",
        )?,
        history_reloaded,
        ORIGIN,
    )?;
    wait_marker(&servo, &history_view, deadline, "/crash-history-source")?;
    ensure!(
        identity(&servo, &history_view, deadline)? == history_reloaded,
        "Reload from the rebuilt crash page activated another document"
    );
    println!(
        "S1 crash-history-reload: PASS (back rebuilt a closed crash entry, reload restored the failed load)"
    );
    drop(history_view);

    // Fail the script thread after response headers. First the body is shorter than Servo's
    // MIME-sniffing buffer, so the active document has no DOM. Then the parser of an active
    // document waits for a withheld classic script. The main response stays open in both.
    for variant in [PartialFailure::BeforeParse, PartialFailure::ParserBlocked] {
        let detail = partial_failure(&servo, context.clone(), deadline, &engine_errors, variant)?;
        println!("S1 {}: PASS ({detail})", variant.name());
    }
    // Fail the script thread of documents that Servo constructs without a navigation fetch,
    // and of documents whose parser waits for a script that `document.write` inserted.
    for variant in [
        Construction::AboutBlank,
        Construction::AboutBlankWrite,
        Construction::Srcdoc,
        Construction::DocumentWrite,
        Construction::OpenWrite,
    ] {
        let detail = construction_failure(
            &servo,
            context.clone(),
            deadline,
            &engine_errors,
            (&failure, recovered),
            variant,
        )?;
        println!("S1 {}: PASS ({detail})", variant.name());
    }
    let detail = javascript_url_failure(&servo, context.clone(), deadline, (&failure, recovered))?;
    println!("S1 javascript-url-crash-recovery: PASS ({detail})");
    // Fail a cross-site navigation's document while its parser waits for a withheld script.
    let detail = cross_site_parse_failure(
        &servo,
        context.clone(),
        deadline,
        &engine_errors,
        multiprocess,
        (&failure, recovered),
        ParseFailure::ScriptThread,
    )?;
    println!("S1 cross-site-parser-blocked-failure: PASS ({detail})");
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
    println!(
        "S1 document-scope documents: PASS ({DOCUMENT_CASES} cases; experimental lifecycle and host gate)"
    );
    Ok(())
}

// The frames part: failures of child frames and their containment.
#[allow(clippy::too_many_arguments)]
fn frame_cases(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    multiprocess: bool,
    (failure, recovered): (&WebView, DocumentIdentity),
) -> Result<()> {
    for (frame, name) in [
        (
            FrameParse::CrossSite,
            "cross-site-subframe-parser-blocked-failure",
        ),
        (
            FrameParse::SandboxedSrcdoc,
            "sandboxed-srcdoc-parser-blocked-failure",
        ),
    ] {
        let detail = frame_parse_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            frame,
            ParseFailure::ScriptThread,
        )?;
        println!("S1 {name}: PASS ({detail})");
    }
    // Fail the event loop of a child frame's pending navigation before its response headers.
    for (site, name) in [
        (
            FrameSite::SameSite,
            "same-site-frame-pending-navigation-failure",
        ),
        (
            FrameSite::CrossSite,
            "cross-site-frame-pending-navigation-failure",
        ),
        (
            FrameSite::CrossSiteAlone,
            "cross-site-lone-frame-pending-navigation-failure",
        ),
    ] {
        let detail = frame_pending_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            site,
            ParseFailure::ScriptThread,
        )?;
        println!("S1 {name}: PASS ({detail})");
    }
    // Fail a frame's first navigation, a loaded child with a healthy pending navigation, and a
    // loaded child with an earlier frame document in history.
    for (path, name) in [
        (FramePath::Initial, "frame-initial-navigation-failure"),
        (
            FramePath::HealthyPending,
            "frame-healthy-pending-navigation-failure",
        ),
        (FramePath::History, "frame-history-failure"),
    ] {
        let detail = frame_path_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            path,
            ParseFailure::ScriptThread,
        )?;
        println!("S1 {name}: PASS ({detail})");
    }
    Ok(())
}

// The processes part: content-process exit and kill. It runs only in multiprocess mode.
fn process_cases(
    servo: &Servo,
    context: Rc<dyn RenderingContext>,
    deadline: Instant,
    engine_errors: &BackendErrors,
    (failure, failure_delegate, recovered): (&WebView, &Delegate, DocumentIdentity),
) -> Result<()> {
    let multiprocess = true;
    for (kill, name, path, how) in [
        (
            false,
            "content-process-failure",
            "process-failure",
            "child exit",
        ),
        (true, "content-process-kill", "process-kill", "host SIGKILL"),
    ] {
        let process_delegate = Rc::new(Delegate::default());
        let process_view = WebViewBuilder::new(servo, context.clone())
            .url(url::Url::parse(&format!("{ORIGIN}/{path}"))?)
            .delegate(process_delegate.clone())
            .build();
        drive_until(servo, deadline, || process_delegate.loads.get() > 0)?;
        let process_document = identity(servo, &process_view, deadline)?;
        let process_ticket = process_delegate.ticket(process_document)?;
        let process_reply = scoped(
            servo,
            &process_view,
            deadline,
            process_document,
            "'before-process-exit'",
        )?;
        ensure!(process_reply.is_ok(), "Process failure control failed");
        let (shared, shared_delegate, shared_document) = open_popup(
            servo,
            &process_view,
            &process_delegate,
            context.clone(),
            deadline,
            process_document,
        )?;
        let process_id = process_view.content_process_id_for_document_scope_probe();
        ensure!(
            process_id != std::process::id()
                && shared.content_process_id_for_document_scope_probe() == process_id,
            "Popup did not share the opener's content process"
        );
        let shared_ticket = shared_delegate.ticket(shared_document)?;
        let shared_reply = scoped(servo, &shared, deadline, shared_document, "'shared'")?;
        ensure!(shared_reply.is_ok(), "Shared process control failed");

        // Retain the final host handle of a third view through ordinary close.
        // In kill mode, close it while the child is blocked, then kill that child
        // before it can acknowledge pipeline exit. This orders the teardown race.
        let (closing, closing_delegate, closing_document) = open_popup(
            servo,
            &process_view,
            &process_delegate,
            context.clone(),
            deadline,
            process_document,
        )?;
        ensure!(
            closing.content_process_id_for_document_scope_probe() == process_id,
            "Closing sibling did not share the content process"
        );
        let closing_ticket = closing_delegate.ticket(closing_document)?;
        let mut closing_reply = Some(scoped(
            servo,
            &closing,
            deadline,
            closing_document,
            "'before-close'",
        )?);
        if !kill {
            let closing_audit =
                process_view.with_paused_content_process_for_document_scope_probe(|| {
                    let audit = CleanupAudit::queue(
                        &closing,
                        Some(closing_document),
                        &closing_delegate,
                        true,
                    );
                    closing.close_for_document_scope_probe();
                    audit
                });
            drive_until(servo, deadline, || closing_delegate.closed.get())?;
            closing_audit.check()?;
            closing_delegate.reject(closing_ticket, closing_reply.take().unwrap())?;
            text(
                scoped(servo, &shared, deadline, shared_document, "'still-live'")?,
                shared_document,
                "still-live",
            )?;
            text(
                scoped(
                    servo,
                    &process_view,
                    deadline,
                    process_document,
                    "'opener-still-live'",
                )?,
                process_document,
                "opener-still-live",
            )?;
            ensure!(
                process_delegate.active.get() == Some(process_document)
                    && shared_delegate.active.get() == Some(shared_document)
                    && !closing_delegate.crashed.get()
                    && !process_delegate.crashed.get()
                    && !shared_delegate.crashed.get(),
                "Ordinary shared-process close entered crash recovery"
            );
            println!(
                "S1 shared-process-close: PASS (8 callbacks released, two siblings retain scope and remain usable)"
            );
        }
        let queue = || {
            let process_audit = CleanupAudit::queue(
                &process_view,
                Some(process_document),
                &process_delegate,
                true,
            );
            let shared_audit =
                CleanupAudit::queue(&shared, Some(shared_document), &shared_delegate, true);
            let closing_audit = kill.then(|| {
                let audit =
                    CleanupAudit::queue(&closing, Some(closing_document), &closing_delegate, true);
                assert_eq!(closing.pending_evaluations_for_document_scope_probe(), 8);
                closing.close_for_document_scope_probe();
                audit
            });
            (process_audit, shared_audit, closing_audit)
        };
        // Kill mode leaves the child no exit path of its own. The host sends SIGKILL.
        let (process_audit, shared_audit, closing_audit) = if kill {
            process_view.with_killed_content_process_for_document_scope_probe(queue)
        } else {
            process_view.with_terminating_content_process_for_document_scope_probe(queue)
        };
        ensure!(
            process_view.pending_evaluations_for_document_scope_probe() == 8
                && shared.pending_evaluations_for_document_scope_probe() == 8,
            "Process failure fixture did not retain eight callbacks per view"
        );
        // Do not send work to the dead process. Only its lifeline can trigger recovery.
        drive_until(
            servo,
            deadline.min(Instant::now() + Duration::from_secs(5)),
            || process_delegate.crashed.get() && shared_delegate.crashed.get(),
        )
        .map_err(|error| {
            anyhow::anyhow!(
                "Content lifeline recovery failed: {error}; active={:?}, pending={}, calls={}",
                process_delegate.active.get(),
                process_view.pending_evaluations_for_document_scope_probe(),
                process_audit.calls.get()
            )
        })?;
        ensure!(
            process_delegate.crash_reasons.borrow().as_slice() == ["Content process disconnected"],
            "Process failure did not use lifeline recovery: {:?}",
            process_delegate.crash_reasons.borrow()
        );
        process_audit.check()?;
        shared_audit.check()?;
        ensure!(
            shared_delegate.crash_reasons.borrow().as_slice() == ["Content process disconnected"]
                && shared.pending_evaluations_for_document_scope_probe() == 0,
            "Shared view missed lifeline cleanup: {:?}",
            shared_delegate.crash_reasons.borrow()
        );
        shared_delegate.reject(shared_ticket, shared_reply)?;
        ensure!(
            process_view.pending_evaluations_for_document_scope_probe() == 0,
            "Process failure retained callbacks"
        );
        process_delegate.reject(process_ticket, process_reply)?;
        drive_until(servo, deadline, || {
            process_delegate
                .active
                .get()
                .is_some_and(|id| id != process_document)
        })?;
        let crash_document = identity(servo, &process_view, deadline)?;
        crash_page(
            servo,
            &process_view,
            deadline,
            crash_document,
            "Content process disconnected",
        )?;
        load(servo, &process_view, deadline, &format!("/after-{path}"))?;
        let process_recovered = identity(servo, &process_view, deadline)?;
        ensure!(
            process_recovered != process_document,
            "Process recovery revived old scope"
        );
        stale(scoped(
            servo,
            &process_view,
            deadline,
            process_document,
            "'obsolete'",
        )?)?;
        text(
            scoped(
                servo,
                &process_view,
                deadline,
                process_recovered,
                "'recovered'",
            )?,
            process_recovered,
            "recovered",
        )?;
        drive_until(servo, deadline, || {
            shared_delegate
                .active
                .get()
                .is_some_and(|id| id != shared_document)
        })?;
        let shared_crash_document = identity(servo, &shared, deadline)?;
        // Servo loads about:blank synchronously and skips the crash fetch, so this
        // crash document is blank. The product's native error state must cover it.
        text(
            scoped(
                servo,
                &shared,
                deadline,
                shared_crash_document,
                "location.href + ' ' + document.documentElement.textContent.includes('Content process disconnected')",
            )?,
            shared_crash_document,
            "about:blank false",
        )?;
        load(servo, &shared, deadline, &format!("/shared-after-{path}"))?;
        let shared_recovered = identity(servo, &shared, deadline)?;
        ensure!(
            shared_recovered != shared_document,
            "Shared recovery revived old scope"
        );
        stale(scoped(
            servo,
            &shared,
            deadline,
            shared_document,
            "'obsolete'",
        )?)?;
        text(
            scoped(
                servo,
                &shared,
                deadline,
                shared_recovered,
                "'shared-recovered'",
            )?,
            shared_recovered,
            "shared-recovered",
        )?;
        ensure!(
            failure_delegate.active.get() == Some(recovered),
            "Process failure revoked an independent view"
        );
        text(
            scoped(servo, failure, deadline, recovered, "'survivor'")?,
            recovered,
            "survivor",
        )?;
        ensure!(
            process_audit.calls.get() == 8 && shared_audit.calls.get() == 8,
            "Process recovery repeated callbacks"
        );
        println!(
            "S1 {name}: PASS ({how}, lifeline revocation, 8 callbacks released, recovery and independent survivor)"
        );
        let shared_name = if kill {
            "shared-process-kill"
        } else {
            "shared-process-failure"
        };
        println!(
            "S1 {shared_name}: PASS (engine-reported shared PID, 16 callbacks released, both views recovered, independent survivor)"
        );
        if let Some(audit) = closing_audit {
            audit.check()?;
            closing_delegate.reject(closing_ticket, closing_reply.take().unwrap())?;
            ensure!(
                closing_delegate.closed.get()
                    && !closing_delegate.crashed.get()
                    && closing_delegate.active.get().is_none()
                    && closing.pending_evaluations_for_document_scope_probe() == 0,
                "Closed sibling entered crash recovery after lifeline failure: {:?}",
                closing_delegate.crash_reasons.borrow()
            );
            println!(
                "S1 shared-process-close-kill-race: PASS (close precedes kill, 8 callbacks released, closed view stays revoked without crash recovery)"
            );
        }
    }

    // A child that hosts only a pending pipeline fails before any document exists.
    for (kill, kind) in [(false, "exit"), (true, "kill")] {
        let terminate = |view: &WebView| {
            if kill {
                view.with_killed_pending_content_process_for_document_scope_probe(|| ());
            } else {
                view.with_terminating_pending_content_process_for_document_scope_probe(|| ());
            }
        };
        let lifeline = || deadline.min(Instant::now() + Duration::from_secs(5));

        let unbuilt_delegate = Rc::new(Delegate::default());
        let unbuilt = WebViewBuilder::new(servo, context.clone())
            .url(url::Url::parse(&format!("{ORIGIN}/held-process-{kind}"))?)
            .delegate(unbuilt_delegate.clone())
            .build();
        drive_until(servo, deadline, || {
            !unbuilt_delegate.held.borrow().is_empty()
        })?;
        terminate(&unbuilt);
        // Send no further work. Only the lifeline can report this failure.
        drive_until(servo, lifeline(), || {
            unbuilt_delegate.crashed.get() && unbuilt_delegate.events.borrow().len() >= 2
        })
        .map_err(|error| {
            anyhow::anyhow!(
                "Unconstructed process {kind} missed lifeline recovery: {error}; events={:?}, crashes={:?}",
                unbuilt_delegate.events.borrow(),
                unbuilt_delegate.crash_reasons.borrow()
            )
        })?;
        ensure!(
            unbuilt_delegate.crash_reasons.borrow().as_slice() == ["Content process disconnected"]
                && unbuilt_delegate.events.borrow().as_slice()
                    == [DocumentLifecycle::Active(None); 2]
                && unbuilt_delegate.active.get().is_none(),
            "Unconstructed process {kind} reported the wrong failure: events={:?}, crashes={:?}",
            unbuilt_delegate.events.borrow(),
            unbuilt_delegate.crash_reasons.borrow()
        );
        ensure!(
            cancel_held(&unbuilt_delegate) == 1,
            "Expected one withheld navigation"
        );
        unbuilt.close_for_document_scope_probe();
        drive_until(servo, deadline, || unbuilt_delegate.closed.get())?;
        drop(unbuilt);
        ensure!(
            unbuilt_delegate.released.get(),
            "Final release of the failed view did not notify the host"
        );
        println!(
            "S1 unconstructed-process-{kind}: PASS (lifeline crash notification, revocation, view closed)"
        );

        // The active document lives in another child. The barrier holds its callbacks.
        let pending_delegate = Rc::new(Delegate::default());
        let pending_view = WebViewBuilder::new(servo, context.clone())
            .url(url::Url::parse(&format!("{ORIGIN}/process-source-{kind}"))?)
            .delegate(pending_delegate.clone())
            .build();
        drive_until(servo, deadline, || pending_delegate.loads.get() > 0)?;
        let source = identity(servo, &pending_view, deadline)?;
        let source_ticket = pending_delegate.ticket(source)?;
        let source_reply = scoped(servo, &pending_view, deadline, source, "'before'")?;
        ensure!(source_reply.is_ok(), "Pending process control failed");
        pending_view.load(url::Url::parse(&format!(
            "https://pending.invalid/held-process-navigation-{kind}"
        ))?);
        drive_until(servo, deadline, || {
            !pending_delegate.held.borrow().is_empty()
        })?;
        let (audit, revoked) = pending_view.with_paused_script_for_document_scope_probe(|| {
            let audit = CleanupAudit::queue(&pending_view, Some(source), &pending_delegate, true);
            terminate(&pending_view);
            let revoked = drive_until(
                servo,
                deadline.min(Instant::now() + Duration::from_secs(4)),
                || audit.calls.get() == 8,
            );
            (audit, revoked)
        });
        revoked.map_err(|error| {
            anyhow::anyhow!(
                "Pending process {kind} did not revoke within the barrier: {error}; active={:?}, crashes={:?}, calls={}",
                pending_delegate.active.get(),
                pending_delegate.crash_reasons.borrow(),
                audit.calls.get()
            )
        })?;
        audit.check()?;
        ensure!(
            pending_delegate.crash_reasons.borrow().as_slice() == ["Content process disconnected"]
                && pending_view.pending_evaluations_for_document_scope_probe() == 0,
            "Pending process {kind} reported the wrong failure: {:?}",
            pending_delegate.crash_reasons.borrow()
        );
        pending_delegate.reject(source_ticket, source_reply)?;
        ensure!(
            cancel_held(&pending_delegate) == 1,
            "Expected one withheld navigation"
        );
        drive_until(servo, deadline, || {
            pending_delegate.active.get().is_some_and(|id| id != source)
        })?;
        let crash_document = identity(servo, &pending_view, deadline)?;
        crash_page(
            servo,
            &pending_view,
            deadline,
            crash_document,
            "Content process disconnected",
        )?;
        // The failed navigation must not block a new one.
        load(
            servo,
            &pending_view,
            deadline,
            &format!("/after-process-{kind}"),
        )?;
        let recovered_view = identity(servo, &pending_view, deadline)?;
        stale(scoped(
            servo,
            &pending_view,
            deadline,
            source,
            "'obsolete'",
        )?)?;
        text(
            scoped(
                servo,
                &pending_view,
                deadline,
                recovered_view,
                "'recovered'",
            )?,
            recovered_view,
            "recovered",
        )?;
        println!(
            "S1 pending-navigation-process-{kind}: PASS (lifeline revocation, 8 callbacks released, crash document, navigation recovers)"
        );
    }

    // A child exits or is killed while a top-level document or a child frame in it parses.
    for (mode, kind) in [
        (ParseFailure::ProcessExit, "exit"),
        (ParseFailure::ProcessKill, "kill"),
    ] {
        let detail = cross_site_parse_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            mode,
        )?;
        println!("S1 cross-site-parser-blocked-process-{kind}: PASS ({detail})");
        let detail = frame_parse_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            FrameParse::CrossSite,
            mode,
        )?;
        println!("S1 cross-site-subframe-process-{kind}: PASS ({detail})");
    }

    // A child exits or is killed while a sandboxed srcdoc child in it parses, or while it
    // holds a child frame's pending pipeline.
    for (mode, kind) in [
        (ParseFailure::ProcessExit, "exit"),
        (ParseFailure::ProcessKill, "kill"),
    ] {
        let detail = frame_parse_failure(
            servo,
            context.clone(),
            deadline,
            engine_errors,
            multiprocess,
            (failure, recovered),
            FrameParse::SandboxedSrcdoc,
            mode,
        )?;
        println!("S1 sandboxed-srcdoc-process-{kind}: PASS ({detail})");
        for (site, name) in [
            (FrameSite::SameSite, "same-site"),
            (FrameSite::CrossSite, "cross-site"),
            (FrameSite::CrossSiteAlone, "cross-site-lone"),
        ] {
            let detail = frame_pending_failure(
                servo,
                context.clone(),
                deadline,
                engine_errors,
                multiprocess,
                (failure, recovered),
                site,
                mode,
            )?;
            println!("S1 {name}-frame-pending-process-{kind}: PASS ({detail})");
        }
    }
    // A child exits or is killed in the three frame paths above.
    for (mode, kind) in [
        (ParseFailure::ProcessExit, "exit"),
        (ParseFailure::ProcessKill, "kill"),
    ] {
        for (path, name) in [
            (FramePath::Initial, "frame-initial-navigation"),
            (FramePath::HealthyPending, "frame-healthy-pending"),
            (FramePath::History, "frame-history"),
        ] {
            let detail = frame_path_failure(
                servo,
                context.clone(),
                deadline,
                engine_errors,
                multiprocess,
                (failure, recovered),
                path,
                mode,
            )?;
            println!("S1 {name}-process-{kind}: PASS ({detail})");
        }
    }
    Ok(())
}

// Case counts of each part. The runner checks each part and the total.
const DOCUMENT_CASES: usize = 51;
const FRAME_CASES: usize = 8;
const PROCESS_CASES: usize = 28;

// Each part runs in its own engine process, so each part has its own deadline.
pub(super) fn run(multiprocess: bool, part: Option<&str>) -> Result<()> {
    match part {
        Some("documents") => run_documents(multiprocess),
        Some("frames") => run_part(multiprocess, false),
        Some("processes") if multiprocess => run_part(multiprocess, true),
        Some("processes") => anyhow::bail!("The processes part requires multiprocess mode"),
        _ => anyhow::bail!("Select a document-scope part: documents, frames, or processes"),
    }
}

// The frames and processes parts start their own engine. A fresh view at the fixture origin
// is the independent survivor that each case checks.
fn run_part(multiprocess: bool, processes: bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(if multiprocess { 80 } else { 30 });
    let (context, servo) = start(multiprocess)?;
    // Engine errors are observations only. A case asserts on them only where it says so.
    let engine_errors = Rc::new(BackendErrors::default());
    servo.set_delegate(engine_errors.clone());
    let survivor_delegate = Rc::new(Delegate::default());
    let survivor = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/independent-survivor"))?)
        .delegate(survivor_delegate.clone())
        .build();
    survivor.show();
    drive_until(&servo, deadline, || survivor_delegate.loads.get() > 0)?;
    let survivor_document = identity(&servo, &survivor, deadline)?;
    let (name, count) = if processes {
        process_cases(
            &servo,
            context,
            deadline,
            &engine_errors,
            (&survivor, &survivor_delegate, survivor_document),
        )?;
        ("processes", PROCESS_CASES)
    } else {
        frame_cases(
            &servo,
            context,
            deadline,
            &engine_errors,
            multiprocess,
            (&survivor, survivor_document),
        )?;
        ("frames", FRAME_CASES)
    };
    ensure!(
        survivor_delegate.active.get() == Some(survivor_document)
            && !survivor_delegate.crashed.get(),
        "The {name} part revoked the independent survivor"
    );
    text(
        scoped(
            &servo,
            &survivor,
            deadline,
            survivor_document,
            "'part-survivor'",
        )?,
        survivor_document,
        "part-survivor",
    )?;
    servo.start_shutdown_for_document_scope_probe();
    drive_until(&servo, deadline, || {
        servo.shutdown_complete_for_document_scope_probe()
    })?;
    println!(
        "S1 document-scope {name}: PASS ({count} cases; experimental lifecycle and host gate)"
    );
    Ok(())
}

#[derive(Default)]
struct BackendErrors(RefCell<Vec<String>>);
impl ServoDelegate for BackendErrors {
    fn notify_error(&self, error: ServoError) {
        self.0.borrow_mut().push(format!("{error:?}"));
    }
}

// Backend loss ends the engine, so this audit needs its own process and engine.
pub(super) fn run_backend(multiprocess: bool) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(if multiprocess { 60 } else { 25 });
    let (context, servo) = start(multiprocess)?;
    let errors = Rc::new(BackendErrors::default());
    servo.set_delegate(errors.clone());
    let delegate = Rc::new(Delegate::default());
    let view = WebViewBuilder::new(&servo, context.clone())
        .url(url::Url::parse(&format!("{ORIGIN}/backend"))?)
        .delegate(delegate.clone())
        .build();
    let other_delegate = Rc::new(Delegate::default());
    let other = WebViewBuilder::new(&servo, context)
        .url(url::Url::parse(&format!("{ORIGIN}/backend-other"))?)
        .delegate(other_delegate.clone())
        .build();
    drive_until(&servo, deadline, || {
        delegate.loads.get() > 0 && other_delegate.loads.get() > 0
    })?;
    let document = identity(&servo, &view, deadline)?;
    let other_document = identity(&servo, &other, deadline)?;
    ensure!(
        other_delegate.active.get() == Some(other_document),
        "Second view activation missing"
    );
    let ticket = delegate.ticket(document)?;
    let reply = scoped(&servo, &view, deadline, document, "'before-backend-loss'")?;
    ensure!(reply.is_ok(), "Backend loss control failed");
    // The paused script thread cannot answer before the constellation unwinds.
    // The constellation forwards all eight requests before it reads the crash message.
    let mut crashed = Instant::now();
    let audit = view.with_paused_script_for_document_scope_probe(|| {
        let audit = CleanupAudit::queue(&view, Some(document), &delegate, true);
        crashed = Instant::now();
        servo.crash_constellation_for_document_scope_probe();
        audit
    });
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 8,
        "Backend loss fixture did not retain eight callbacks"
    );
    // Send no further work. Only the closed constellation channel can trigger cleanup.
    drive_until(
        &servo,
        deadline.min(Instant::now() + Duration::from_secs(5)),
        || audit.calls.get() == 8,
    )
    .map_err(|error| {
        anyhow::anyhow!(
            "Backend loss cleanup failed: {error}; active={:?}, pending={}, calls={}, errors={:?}",
            delegate.active.get(),
            view.pending_evaluations_for_document_scope_probe(),
            audit.calls.get(),
            errors.0.borrow()
        )
    })?;
    let cleanup = crashed.elapsed();
    audit.check()?;
    ensure!(
        other_delegate.active.get().is_none(),
        "Backend loss left another view active"
    );
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0,
        "Backend loss retained callbacks"
    );
    delegate.reject(ticket, reply)?;
    // Admission after loss fails synchronously and retains no callback.
    let (sender, after) = mpsc::channel();
    view.evaluate_javascript_in_document(document, "'after-backend-loss'", move |result| {
        let _ = sender.send(result);
    });
    stale(after.try_recv()?)?;
    let (sender, legacy) = mpsc::channel();
    other.evaluate_javascript("'after-backend-loss'", move |result| {
        let _ = sender.send(result);
    });
    ensure!(
        matches!(
            legacy.try_recv()?,
            Err(JavaScriptEvaluationError::StaleDocument)
        ),
        "Legacy evaluator admitted work after backend loss"
    );
    ensure!(
        view.pending_evaluations_for_document_scope_probe() == 0
            && other.pending_evaluations_for_document_scope_probe() == 0
            && audit.calls.get() == 8,
        "Backend loss admitted or repeated callbacks"
    );
    println!(
        "S1 backend-disconnection: PASS (constellation unwind, 2 views revoked, 8 callbacks released, admission closed; cleanup after {} ms)",
        cleanup.as_millis()
    );

    // The destructor previously waited for a completion message that a lost constellation cannot send.
    // Check completion after the destructor so that the watchdog reports that wait directly.
    for _ in 0..10 {
        servo.spin_event_loop();
    }
    let complete = servo.shutdown_complete_for_document_scope_probe();
    let (finished, watchdog) = mpsc::channel::<()>();
    let watchdog = thread::spawn(move || {
        if watchdog.recv_timeout(Duration::from_secs(5)) == Err(mpsc::RecvTimeoutError::Timeout) {
            eprintln!("Engine drop waited for the lost constellation");
            std::process::exit(1);
        }
    });
    let dropping = Instant::now();
    drop((view, other));
    drop(servo);
    let _ = finished.send(());
    let _ = watchdog.join();
    ensure!(complete, "Backend loss left engine shutdown incomplete");
    ensure!(
        errors.0.borrow().as_slice() == ["LostConnectionWithBackend"],
        "Expected one backend loss report: {:?}",
        errors.0.borrow()
    );
    println!(
        "S1 drop-after-backend-loss: PASS (local shutdown completion, one loss report, destructor returned after {} ms)",
        dropping.elapsed().as_millis()
    );
    println!("S1 backend-loss prototype: PASS (2 cases; constellation unwind)");
    Ok(())
}
