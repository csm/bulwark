//! The only Rust-populated Clojure namespace: browser futures and key reservations.
use bulwark_engine::{
    Command, Data, Event, EventKind, Handle,
    bridge::{Client, DEFAULT_TIMEOUT, RuntimeEndpoint},
};
use cljrs_gc::GcPtr;
use cljrs_runtime::env::env::GlobalEnv;
use cljrs_value::{Arity, Keyword, MapValue, NativeFn, Value, ValueError};
use std::{rc::Rc, sync::Arc};
use tokio::sync::Mutex;

pub(crate) fn keyword(name: &str) -> Value {
    Value::keyword(match name.split_once('/') {
        Some((ns, name)) => Keyword::qualified(ns, name),
        None => Keyword::simple(name),
    })
}
pub(crate) fn map(fields: Vec<(&str, Value)>) -> Value {
    Value::Map(MapValue::from_pairs(
        fields.into_iter().map(|(k, v)| (keyword(k), v)).collect(),
    ))
}
fn info(info: bulwark_engine::ViewInfo) -> Value {
    map(vec![
        ("url", Value::string(info.url)),
        ("title", Value::string(info.title)),
        ("load-status", keyword(info.load_status.name())),
        ("can-go-back", Value::Bool(info.can_go_back)),
        ("can-go-forward", Value::Bool(info.can_go_forward)),
    ])
}
fn response(id: Option<u64>, result: Result<Data, bulwark_engine::Fault>) -> Value {
    let mut fields = vec![(
        "request/id",
        id.map_or(Value::Nil, |id| Value::Long(id as i64)),
    )];
    match result {
        Ok(data) => fields.extend([
            ("ok", Value::Bool(true)),
            (
                "value",
                match data {
                    Data::Nil => Value::Nil,
                    Data::Handle(id) => Value::Long(id.0 as i64),
                    Data::Text(text) => Value::string(text),
                    Data::Info(value) => info(value),
                },
            ),
        ]),
        Err(error) => {
            fields.extend([("ok", Value::Bool(false)), ("error", keyword(error.kind()))]);
            if let bulwark_engine::Fault::Engine(message) = error {
                fields.push(("message", Value::string(message)));
            }
        }
    }
    map(fields)
}
fn event_value(event: Event) -> Value {
    let mut fields = vec![
        ("sequence", Value::Long(event.sequence as i64)),
        ("event/type", keyword(event.kind.name())),
    ];
    // Frame events belong to no webview.
    if event.view != Handle::NONE {
        fields.push(("webview", Value::Long(event.view.0 as i64)));
    }
    let interaction = |element: u64, revision: u64| {
        [
            ("ui/id", Value::Long(element as i64)),
            ("revision", Value::Long(revision as i64)),
        ]
    };
    match event.kind {
        EventKind::UrlChanged(url) => fields.push(("url", Value::string(url))),
        EventKind::TitleChanged(title) => fields.push(("title", Value::string(title))),
        EventKind::LoadStatusChanged(status) => {
            fields.push(("load-status", keyword(status.name())))
        }
        EventKind::HistoryChanged {
            can_go_back,
            can_go_forward,
        } => fields.extend([
            ("can-go-back", Value::Bool(can_go_back)),
            ("can-go-forward", Value::Bool(can_go_forward)),
        ]),
        EventKind::Crashed(message) => fields.push(("message", Value::string(message))),
        EventKind::KeyInput(key) => fields.extend([
            ("key", Value::string(key.key)),
            ("control", Value::Bool(key.control)),
            ("alt", Value::Bool(key.alt)),
            ("shift", Value::Bool(key.shift)),
            ("meta", Value::Bool(key.meta)),
            ("pressed", Value::Bool(key.pressed)),
        ]),
        EventKind::UiPressed { element, revision } => fields.extend(interaction(element, revision)),
        EventKind::PopupRequested { request } => {
            fields.push(("popup/request", Value::Long(request.0 as i64)))
        }
        EventKind::FrameFocusChanged(focused) => fields.push(("focused", Value::Bool(focused))),
        EventKind::UiInputChanged {
            element,
            revision,
            text,
        }
        | EventKind::UiInputSubmitted {
            element,
            revision,
            text,
        } => {
            fields.extend(interaction(element, revision));
            fields.push(("text", Value::string(text)));
        }
        EventKind::Closed | EventKind::InputReset | EventKind::ViewportChanged => {}
    }
    map(fields)
}

fn text(value: &Value) -> Result<String, ValueError> {
    match value {
        Value::Str(s) => Ok(s.get().clone()),
        _ => Err(ValueError::Other("Expected a string".into())),
    }
}
fn handle(value: &Value) -> Result<Handle, ValueError> {
    match value {
        Value::Long(id) if *id > 0 => Ok(Handle(*id as u64)),
        _ => Err(ValueError::Other(
            "Expected a positive webview handle".into(),
        )),
    }
}
fn invalid(message: &str) -> ValueError {
    ValueError::Other(message.into())
}
fn field(map: &Value, name: &str) -> Option<Value> {
    match map {
        Value::Map(map) => map.get(&keyword(name)).filter(|v| !matches!(v, Value::Nil)),
        _ => None,
    }
}
fn number(map: &Value, name: &str) -> Result<Option<u64>, ValueError> {
    match field(map, name) {
        None => Ok(None),
        Some(Value::Long(n)) if n >= 0 => Ok(Some(n as u64)),
        Some(_) => Err(invalid("UI numbers must be nonnegative integers")),
    }
}
fn extent(map: &Value, name: &str) -> Result<u32, ValueError> {
    number(map, name)?
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| invalid("A UI element needs integer :x, :y, :width, and :height"))
}
/// Copies a Clojure UI description into owned, validated data.
fn ui(description: &Value) -> Result<bulwark_engine::ui::Ui, ValueError> {
    use bulwark_engine::ui::{Element, Rect, Ui, Widget};
    let elements = match field(description, "elements") {
        Some(Value::Vector(elements)) => elements.get().iter().cloned().collect::<Vec<_>>(),
        _ => return Err(invalid("A UI description needs an :elements vector")),
    };
    if elements.len() > bulwark_engine::ui::MAX_ELEMENTS {
        return Err(invalid("UI description has more than 64 elements"));
    }
    let elements = elements
        .iter()
        .map(|element| {
            let content = || field(element, "text").map_or(Ok(String::new()), |v| text(&v));
            let kind = field(element, "ui/kind");
            let widget = if kind == Some(keyword("web")) {
                Widget::Web(handle(&field(element, "webview").unwrap_or(Value::Nil))?)
            } else if kind == Some(keyword("text")) {
                Widget::Text(content()?)
            } else if kind == Some(keyword("line")) {
                Widget::Line {
                    text: content()?,
                    emphasis: field(element, "emphasis") == Some(Value::Bool(true)),
                }
            } else if kind == Some(keyword("echo")) {
                Widget::Echo
            } else if kind == Some(keyword("input")) {
                Widget::Input {
                    text: content()?,
                    label: field(element, "label").map_or(Ok(String::new()), |v| text(&v))?,
                }
            } else {
                return Err(invalid(
                    "A UI element's :ui/kind must be :web, :text, :line, :echo, or :input",
                ));
            };
            Ok(Element {
                id: number(element, "ui/id")?,
                rect: Rect {
                    x: extent(element, "x")?,
                    y: extent(element, "y")?,
                    width: extent(element, "width")?,
                    height: extent(element, "height")?,
                },
                widget,
            })
        })
        .collect::<Result<Vec<_>, ValueError>>()?;
    Ui::new(
        number(description, "revision")?.unwrap_or(0),
        number(description, "focus")?,
        elements,
    )
    .map_err(invalid)
}
fn request(client: &Client, command: Command) -> Value {
    let pending = client.submit(command, DEFAULT_TIMEOUT);
    cljrs_async::spawn_future(async move {
        Ok(match pending {
            Ok(pending) => {
                let id = pending.id;
                response(Some(id), pending.wait().await)
            }
            Err(error) => response(None, Err(error)),
        })
    })
}

/// Install inside the runtime's LocalSet. Captured bridge state contains no GC values.
pub fn install(globals: &Arc<GlobalEnv>, endpoint: RuntimeEndpoint) {
    let RuntimeEndpoint {
        client,
        events,
        control,
    } = endpoint;
    type BuildCommand = fn(&[Value]) -> Result<Command, ValueError>;
    let operations: &[(&str, usize, BuildCommand)] = &[
        ("webview-create", 1, |a| {
            Ok(Command::Create { url: text(&a[0])? })
        }),
        ("webview-close", 1, |a| Ok(Command::Close(handle(&a[0])?))),
        ("webview-accept-popup", 1, |a| {
            Ok(Command::AcceptPopup(handle(&a[0])?))
        }),
        ("webview-deny-popup", 1, |a| {
            Ok(Command::DenyPopup(handle(&a[0])?))
        }),
        ("webview-zoom", 2, |a| {
            let factor = match a[1] {
                Value::Long(n) => n as f64,
                Value::Double(n) => n,
                _ => return Err(invalid("Zoom must be a finite number from 0.25 to 5")),
            };
            if !factor.is_finite() || !(0.25..=5.0).contains(&factor) {
                return Err(invalid("Zoom must be a finite number from 0.25 to 5"));
            }
            Ok(Command::Zoom {
                view: handle(&a[0])?,
                factor,
            })
        }),
        ("webview-select", 1, |a| {
            Ok(Command::Select(if matches!(a[0], Value::Nil) {
                None
            } else {
                Some(handle(&a[0])?)
            }))
        }),
        ("navigate", 2, |a| {
            Ok(Command::Navigate {
                view: handle(&a[0])?,
                url: text(&a[1])?,
            })
        }),
        ("reload", 1, |a| Ok(Command::Reload(handle(&a[0])?))),
        ("back", 1, |a| Ok(Command::Back(handle(&a[0])?))),
        ("forward", 1, |a| Ok(Command::Forward(handle(&a[0])?))),
        ("webview-info", 1, |a| Ok(Command::Info(handle(&a[0])?))),
        ("eval-js", 2, |a| {
            Ok(Command::Evaluate {
                view: handle(&a[0])?,
                script: text(&a[1])?,
            })
        }),
        ("log", 1, |a| Ok(Command::Log(text(&a[0])?))),
        ("set-echo!", 1, |a| Ok(Command::SetEcho(text(&a[0])?))),
        ("set-ui!", 1, |a| Ok(Command::SetUi(ui(&a[0])?))),
        ("quit", 0, |_| Ok(Command::Quit)),
    ];
    for &(name, arity, build) in operations {
        let client = client.clone();
        let native = NativeFn::with_closure(
            format!("bulwark.native/{name}"),
            Arity::Fixed(arity),
            move |args| Ok(request(&client, build(args)?)),
        );
        globals.intern(
            "bulwark.native",
            name.into(),
            Value::NativeFunction(GcPtr::new(native)),
        );
    }
    let viewport = control.clone();
    let native = NativeFn::with_closure("bulwark.native/ui-viewport", Arity::Fixed(0), move |_| {
        Ok(viewport.viewport().map_or(Value::Nil, |viewport| {
            map(vec![
                ("width", Value::Long(viewport.width as i64)),
                ("height", Value::Long(viewport.height as i64)),
                ("scale", Value::Double(viewport.scale as f64)),
            ])
        }))
    });
    globals.intern(
        "bulwark.native",
        "ui-viewport".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    // Publication is synchronous: bind-key! must update reservations before it
    // returns, including when a caller immediately starts a synchronous loop.
    let native = NativeFn::with_closure(
        "bulwark.native/set-key-routes!",
        Arity::Variadic { min: 2 },
        move |args| {
            use bulwark_engine::keys::OptionMode;
            if args.len() > 4 {
                return Err(ValueError::Other(
                    "Expected two to four key routing arguments".into(),
                ));
            }
            let option_mode = match args.get(2) {
                None => control.key_routes().option_mode,
                Some(value) if *value == keyword("platform-default") => OptionMode::PlatformDefault,
                Some(value) if *value == keyword("characters") => OptionMode::Characters,
                Some(value) if *value == keyword("meta") => OptionMode::Meta,
                _ => {
                    return Err(ValueError::Other(
                        "Option mode must be :platform-default, :characters, or :meta".into(),
                    ));
                }
            };
            let escape_meta = match args.get(3) {
                None => control.key_routes().escape_meta,
                Some(Value::Bool(enabled)) => *enabled,
                _ => return Err(ValueError::Other("Escape Meta flag must be boolean".into())),
            };
            let vector = |value: &Value, max: usize| match value {
                Value::Vector(v) if v.get().count() <= max => {
                    Ok(v.get().iter().cloned().collect::<Vec<_>>())
                }
                _ => Err(ValueError::Other(
                    "Expected a bounded key routing vector".into(),
                )),
            };
            let stroke = |value: &Value| match value {
                Value::Str(s)
                    if !s.get().is_empty()
                        && s.get().len() <= bulwark_engine::keys::MAX_STROKE_BYTES =>
                {
                    Ok(s.get().clone())
                }
                _ => Err(ValueError::Other(
                    "Expected a nonempty key stroke of at most 128 bytes".into(),
                )),
            };
            let sequences = vector(&args[0], bulwark_engine::keys::MAX_SEQUENCES)?;
            let cancel = vector(&args[1], bulwark_engine::keys::MAX_STROKES)?;
            let sequences = sequences
                .iter()
                .map(|sequence| {
                    vector(sequence, bulwark_engine::keys::MAX_STROKES)?
                        .iter()
                        .map(stroke)
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            let cancel = cancel.iter().map(stroke).collect::<Result<Vec<_>, _>>()?;
            let mut routes = bulwark_engine::keys::KeyRoutes::new(sequences, cancel)
                .map_err(|e| ValueError::Other(e.to_string()))?;
            routes.option_mode = option_mode;
            routes.escape_meta = escape_meta;
            control
                .set_key_routes(routes)
                .map_err(|e| ValueError::Other(e.to_string()))?;
            Ok(Value::Nil)
        },
    );
    globals.intern(
        "bulwark.native",
        "set-key-routes!".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    let native = NativeFn::with_closure("bulwark.native/allocate-id!", Arity::Fixed(0), |_| {
        bulwark_engine::allocate_id()
            .map(|id| Value::Long(id as i64))
            .map_err(|error| ValueError::Other(error.to_string()))
    });
    globals.intern(
        "bulwark.native",
        "allocate-id!".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    let events = Rc::new(Mutex::new(events));
    let native = NativeFn::with_closure("bulwark.native/next-event", Arity::Fixed(0), move |_| {
        let events = events.clone();
        Ok(cljrs_async::spawn_future(async move {
            let mut events = events.lock().await;
            Ok(events.recv().await.map_or(Value::Nil, event_value))
        }))
    });
    globals.intern(
        "bulwark.native",
        "next-event".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    let native = NativeFn::with_closure("bulwark.native/read-source", Arity::Fixed(1), |args| {
        let path = text(&args[0])?;
        // Only owned bytes cross the filesystem parking boundary. GC values stay here.
        let file_path = path.clone();
        let source = cljrs_value::parking::blocking(move || {
            use std::io::Read;
            let file = match std::fs::File::open(&file_path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.to_string()),
            };
            let mut source = String::new();
            file.take(256 * 1024 + 1)
                .read_to_string(&mut source)
                .map_err(|e| e.to_string())?;
            if source.len() > 256 * 1024 {
                return Err("Source exceeds 256 KiB".into());
            }
            Ok(Some(source))
        })
        .map_err(|error| invalid(&format!("{path}: {error}")))?;
        let Some(source) = source else {
            return Ok(Value::Nil);
        };
        let forms = cljrs_reader::Parser::new(source, path.clone())
            .parse_all()
            .map_err(|error| invalid(&format!("{path}: {error}")))?;
        let entries = forms
            .iter()
            .map(|form| {
                let value = cljrs_runtime::builtins::form::form_to_value(form)
                    .map_err(|error| invalid(&error.to_string()))?;
                Ok(map(vec![
                    ("form", value),
                    ("line", Value::Long(form.span.line as i64)),
                    ("column", Value::Long(form.span.col as i64)),
                ]))
            })
            .collect::<Result<Vec<_>, ValueError>>()?;
        Ok(Value::Vector(GcPtr::new(entries.into_iter().collect())))
    });
    globals.intern(
        "bulwark.native",
        "read-source".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    crate::evaluation::install(globals);
    install_metadata(globals);
    globals.mark_loaded("bulwark.native");
}

/// One inventory supplies the metadata used by help and apropos.
fn install_metadata(globals: &GlobalEnv) {
    let operations: &[(&str, &str, &str, &[&str])] = &[
        (
            "webview-zoom",
            "[webview factor]",
            "Sets a webview's page zoom to a finite factor from 0.25 to 5. Returns a reply future. Stale handles leave other views unchanged.",
            &["webview"],
        ),
        (
            "webview-accept-popup",
            "[request]",
            "Consumes an auxiliary webview request and returns its new retained handle in a reply future. Preserves the engine's opener relationship. Caller owns the view. Requests expire after ten seconds or parent close; at most 16 globally and four per parent.",
            &["webview", "popup"],
        ),
        (
            "webview-deny-popup",
            "[request]",
            "Denies and consumes an auxiliary webview request. Returns a reply future. Expired or consumed requests return :stale-handle.",
            &["webview", "popup"],
        ),
        (
            "evaluation-start",
            "[owner source namespace]",
            "Queues trusted Clojure source of at most 4096 bytes in the shared runtime. Returns a job id. At most 32 retained jobs and eight per owner; four admissions per millisecond turn. Jobs of one owner run in order. No listener is opened.",
            &["evaluation"],
        ),
        (
            "evaluation-poll",
            "[job]",
            "Drains a job's bounded output and reads its status and namespace. A terminal poll consumes the id. Each job retains at most 16384 output bytes, with a truncation flag.",
            &["evaluation"],
        ),
        (
            "evaluation-interrupt",
            "[job]",
            "Interrupts one evaluation at evaluator safepoints and wakes its parked waits. Queued work runs no forms. Returns nil; stale ids throw.",
            &["evaluation"],
        ),
        (
            "evaluation-close",
            "[owner]",
            "Cancels an owner's queued and running jobs and discards their output. Other owners and nREPL jobs remain active. Returns nil.",
            &["evaluation"],
        ),
        (
            "webview-create",
            "[url]",
            "Creates a retained webview at URL. Await the reply for its fresh handle. The caller owns and must close it.",
            &["webview"],
        ),
        (
            "webview-close",
            "[webview]",
            "Closes a retained webview and invalidates its handle. Returns a reply future. Repeated close returns :stale-handle.",
            &["webview"],
        ),
        (
            "webview-select",
            "[webview]",
            "Selects a retained webview, or hides all views for nil. Returns a reply future. A stale handle leaves selection unchanged.",
            &["webview"],
        ),
        (
            "navigate",
            "[webview url]",
            "Navigates a retained webview to URL. Returns a reply future. The webview keeps its identity.",
            &["webview"],
        ),
        (
            "reload",
            "[webview]",
            "Reloads a retained webview. Returns a reply future.",
            &["webview"],
        ),
        (
            "back",
            "[webview]",
            "Moves a webview backward in its navigation history. Returns a reply future.",
            &["webview"],
        ),
        (
            "forward",
            "[webview]",
            "Moves a webview forward in its navigation history. Returns a reply future.",
            &["webview"],
        ),
        (
            "webview-info",
            "[webview]",
            "Reads a webview's URL, title, load status, and history flags. Returns a reply future.",
            &["webview"],
        ),
        (
            "eval-js",
            "[webview script]",
            "Evaluates JavaScript in a retained webview. Returns a reply future with engine data. Page results are untrusted data.",
            &["webview"],
        ),
        (
            "log",
            "[text]",
            "Writes text to the host log. Returns a reply future.",
            &["log"],
        ),
        (
            "set-echo!",
            "[text]",
            "Stores text for the frame's echo area. Returns a reply future. An open prompt can cover the echo area.",
            &["frame"],
        ),
        (
            "set-ui!",
            "[description]",
            "Applies a bounded frame description {:revision :focus :elements} in logical pixels. Supports :web, :text, :line, :echo, and labeled :input. Input text changes place the cursor at the end. At most one webview is visible. Returns a reply future. Malformed descriptions throw. Stale handles or revisions leave the frame unchanged.",
            &["frame", "webview"],
        ),
        (
            "quit",
            "[]",
            "Requests host shutdown. Returns a reply future. The host owns resource cleanup and the bounded worker join.",
            &["host"],
        ),
        (
            "ui-viewport",
            "[]",
            "Reads {:width :height :scale} in logical pixels, or nil without a frame. Synchronous. Rearms the viewport event.",
            &["frame"],
        ),
        (
            "set-key-routes!",
            "[sequences cancel-strokes] [sequences cancel-strokes option-mode] [sequences cancel-strokes option-mode escape-meta?]",
            "Synchronously replaces bounded native key reservations, optional Option mode (:platform-default, :characters, :meta), and optional boolean Escape Meta flag. Omitted settings retain their current values. Returns nil. Refusal retains the previous snapshot.",
            &["input"],
        ),
        (
            "allocate-id!",
            "[]",
            "Allocates a positive process-wide resource identifier. Synchronous. Identifiers never repeat or equal a webview handle. Exhaustion throws.",
            &["identifier"],
        ),
        (
            "next-event",
            "[]",
            "Returns a future of the next ordered engine event, or nil after shutdown. Events can name a webview. Only the ordered dispatcher consumes this stream.",
            &["event", "webview"],
        ),
        (
            "read-source",
            "[path]",
            "Reads and parses a UTF-8 source file of at most 256 KiB. Returns a vector of {:form :line :column}, or nil for a missing file. Other IO and parse errors throw with the path. Filesystem IO parks the caller. Runs no code.",
            &["file"],
        ),
    ];
    for &(name, signature, doc, resources) in operations {
        let forms = cljrs_reader::Parser::new(format!("({signature})"), "<native-metadata>".into())
            .parse_all()
            .expect("Static native signature must parse");
        let arglists = cljrs_runtime::builtins::form::form_to_value(&forms[0])
            .expect("Static native signature must be data");
        let var = globals
            .lookup_var_in_ns("bulwark.native", name)
            .expect("Documented native must be installed");
        var.get().set_meta(map(vec![
            ("doc", Value::string(doc)),
            ("arglists", arglists),
            (
                "resources",
                Value::Vector(GcPtr::new(
                    resources.iter().map(|tag| keyword(tag)).collect(),
                )),
            ),
        ]));
    }
}
