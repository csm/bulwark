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
        ("webview", Value::Long(event.view.0 as i64)),
    ];
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
        EventKind::Closed | EventKind::InputReset => {}
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
