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

fn keyword(name: &str) -> Value {
    Value::keyword(match name.split_once('/') {
        Some((ns, name)) => Keyword::qualified(ns, name),
        None => Keyword::simple(name),
    })
}
fn map(fields: Vec<(&str, Value)>) -> Value {
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
        let var = globals.intern(
            "bulwark.native",
            name.into(),
            Value::NativeFunction(GcPtr::new(native)),
        );
        let doc = match name {
            "webview-select" => {
                "Selects one retained webview, or hides all webviews for nil. Returns a future; a stale handle leaves selection unchanged."
            }
            "set-ui!" => {
                "Replaces the frame's UI description {:revision n :focus id :elements [...]}. Each element has :ui/kind (:web, :text, :line, :echo, or :input) and an integer :x, :y, :width, and :height in logical pixels. :web, :text, and :input need a :ui/id; :web needs a :webview handle; :text, :line, and :input take :text; :input also takes a read-only :label. New :input text replaces the user's edit and places the cursor at its end. The :web element becomes the one visible webview. Throws for a malformed or oversized description. Returns a future; :stale-revision or :stale-handle leaves the frame unchanged."
            }
            _ => continue,
        };
        var.get().set_meta(map(vec![("doc", Value::string(doc))]));
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
    let viewport_var = globals.intern(
        "bulwark.native",
        "ui-viewport".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    viewport_var.get().set_meta(map(vec![("doc", Value::string(
        "Synchronously returns the native frame's {:width :height :scale} in logical pixels, or nil without a window. Reading it rearms the :ui/viewport-changed event."
    ))]));
    // Publication is synchronous: bind-key! must update reservations before it
    // returns, including when a caller immediately starts a synchronous loop.
    let native = NativeFn::with_closure(
        "bulwark.native/set-key-routes!",
        Arity::Fixed(2),
        move |args| {
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
            let routes = bulwark_engine::keys::KeyRoutes::new(sequences, cancel)
                .map_err(|e| ValueError::Other(e.to_string()))?;
            control
                .set_key_routes(routes)
                .map_err(|e| ValueError::Other(e.to_string()))?;
            Ok(Value::Nil)
        },
    );
    let routes_var = globals.intern(
        "bulwark.native",
        "set-key-routes!".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    routes_var.get().set_meta(map(vec![("doc", Value::string(
        "Synchronously replaces native key reservations from vectors of canonical sequences and cancellation strokes. Returns nil, or throws without changing the snapshot."
    ))]));
    let native = NativeFn::with_closure("bulwark.native/allocate-id!", Arity::Fixed(0), |_| {
        bulwark_engine::allocate_id()
            .map(|id| Value::Long(id as i64))
            .map_err(|error| ValueError::Other(error.to_string()))
    });
    let id_var = globals.intern(
        "bulwark.native",
        "allocate-id!".into(),
        Value::NativeFunction(GcPtr::new(native)),
    );
    id_var.get().set_meta(map(vec![("doc", Value::string(
        "Synchronously returns a positive process-wide identifier for a runtime-owned resource. Identifiers never repeat and never equal a webview handle; exhaustion throws."
    ))]));
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
    globals.mark_loaded("bulwark.native");
}
