//! Isolated S1 prototype regressions. Requires the experimental Servo patch.
use super::{ORIGIN, Waker, drive_until, evaluate};
use anyhow::{Result, ensure};
use servo::{
    DocumentIdentity, DocumentJavaScriptResult, DocumentLifecycle, JSValue,
    JavaScriptEvaluationError, LoadStatus, RenderingContext, Servo, ServoBuilder,
    SoftwareRenderingContext, TraversalId, WebResourceLoad, WebResourceResponse, WebView,
    WebViewBuilder, WebViewDelegate,
};
use std::{
    cell::{Cell, RefCell},
    io::{Read, Write},
    net::TcpListener,
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
    traversals: RefCell<Vec<TraversalId>>,
    crashed: Cell<bool>,
}

#[derive(Clone, Copy)]
struct Ticket {
    document: DocumentIdentity,
    generation: u64,
    cancelled: bool,
}

impl Delegate {
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
    fn notify_document_lifecycle(&self, _: WebView, event: DocumentLifecycle) {
        if let DocumentLifecycle::Active(document) = event
            && self.active.replace(document) != document
        {
            self.generation
                .set(self.generation.get().checked_add(1).unwrap());
        }
        self.events.borrow_mut().push(event);
    }
    fn notify_traversal_complete(&self, _: WebView, id: TraversalId) {
        self.traversals.borrow_mut().push(id);
    }
    fn notify_closed(&self, _: WebView) {
        self.closed.set(true);
    }
    fn notify_crashed(&self, _: WebView, _: String, _: Option<String>) {
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
        .event_loop_waker(Box::new(Waker(thread::current())))
        .build();
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

    let before_reload = after_delivery;
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
    drive_until(&servo, deadline, || other_delegate.closed.get())?;
    ensure!(
        other_delegate.active.get().is_none(),
        "Close did not revoke identity"
    );
    other_delegate.reject(close_ticket, close_reply)?;
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
    println!("S1 document-scope prototype: PASS (16 cases; experimental lifecycle and host gate)");
    Ok(())
}
