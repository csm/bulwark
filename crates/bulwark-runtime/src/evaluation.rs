//! Bounded, owner-scoped evaluation on the runtime's LocalSet. No listener or UI policy.
use cljrs_runtime::{
    env::{
        env::GlobalEnv,
        gas::{InterruptFlag, InterruptGuard},
    },
    tiered::{Env, eval},
};
use cljrs_value::{Arity, NativeFn, Value, ValueError};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    fmt::Write,
    future::Future,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

const MAX_JOBS: usize = 32;
const MAX_OWNER_JOBS: usize = 8;
const MAX_OUTPUT: usize = 16_384;

#[derive(Default)]
struct Output {
    text: String,
    truncated: bool,
}
impl Write for Output {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let mut end = text.len().min(MAX_OUTPUT.saturating_sub(self.text.len()));
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&text[..end]);
        self.truncated |= end < text.len();
        Ok(())
    }
}
struct Job {
    owner: u64,
    source: Option<String>,
    namespace: Rc<RefCell<String>>,
    output: Rc<RefCell<Output>>,
    status: Rc<Cell<&'static str>>,
    interrupt: Arc<InterruptFlag>,
    closed: bool,
    finished: bool,
}
#[derive(Default)]
struct State {
    jobs: BTreeMap<u64, Job>,
    queue: VecDeque<u64>,
}
fn id(value: &Value) -> Result<u64, ValueError> {
    match value {
        Value::Long(n) if *n > 0 => Ok(*n as u64),
        _ => Err(ValueError::Other(
            "Expected a positive evaluation identifier".into(),
        )),
    }
}
fn text(value: &Value) -> Result<String, ValueError> {
    match value {
        Value::Str(s) => Ok(s.get().clone()),
        _ => Err(ValueError::Other(
            "Expected evaluation source or namespace text".into(),
        )),
    }
}
fn stale() -> ValueError {
    ValueError::Other("Stale evaluation job".into())
}

// The pinned evaluator writes the shared *ns* root for ns/in-ns. Scope that root
// to each fiber poll: macro expansion sees this job's namespace, including after
// a parked ns change, while the dispatcher and nREPL keep their own namespace.
struct NamespaceScope {
    variable: cljrs_gc::GcPtr<cljrs_value::Var>,
    saved: Option<Value>,
    namespace: Rc<RefCell<String>>,
}
impl NamespaceScope {
    fn enter(globals: &GlobalEnv, namespace: Rc<RefCell<String>>) -> Self {
        let variable = globals
            .lookup_var("clojure.core", "*ns*")
            .expect("Runtime namespace Var");
        let saved = variable.get().deref();
        variable.get().bind(Value::Namespace(
            globals.get_or_create_ns(&namespace.borrow()),
        ));
        Self {
            variable,
            saved,
            namespace,
        }
    }
}
impl Drop for NamespaceScope {
    fn drop(&mut self) {
        if let Some(Value::Namespace(space)) = self.variable.get().deref() {
            *self.namespace.borrow_mut() = space.get().name.to_string();
        }
        if let Some(saved) = self.saved.take() {
            self.variable.get().bind(saved);
        }
    }
}

// Evaluator and fiber callbacks require the upstream EvalResult error by value.
#[allow(clippy::result_large_err)]
pub(crate) fn install(globals: &Arc<GlobalEnv>) {
    let state = Rc::new(RefCell::new(State::default()));
    let queue = state.clone();
    let environments = globals.clone();
    let wake = Rc::new(tokio::sync::Notify::new());
    let turns = wake.clone();
    tokio::task::spawn_local(async move {
        loop {
            let runnable = {
                let state = queue.borrow();
                state.queue.iter().any(|id| {
                    let owner = state.jobs[id].owner;
                    !state
                        .jobs
                        .values()
                        .any(|j| j.owner == owner && j.source.is_none() && !j.finished)
                })
            };
            if !runnable {
                turns.notified().await;
                continue;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
            // Leave the executor between batches, including when every owner is busy.
            for _ in 0..4 {
                let next = {
                    let mut state = queue.borrow_mut();
                    let position = state.queue.iter().position(|id| {
                        let owner = state.jobs[id].owner;
                        !state
                            .jobs
                            .values()
                            .any(|j| j.owner == owner && j.source.is_none() && !j.finished)
                    });
                    position.map(|p| {
                        let id = state.queue.remove(p).expect("Queued job");
                        let job = state.jobs.get_mut(&id).expect("Retained job");
                        job.status.set("running");
                        (
                            id,
                            job.source.take().expect("Unstarted source"),
                            job.namespace.clone(),
                            job.output.clone(),
                            job.status.clone(),
                            job.interrupt.clone(),
                        )
                    })
                };
                let Some((id, source, namespace, output, status, interrupt)) = next else {
                    break;
                };
                let globals = environments.clone();
                let queue = queue.clone();
                let finished = turns.clone();
                tokio::task::spawn_local(async move {
                    let cancelled = interrupt.clone();
                    let failure_status = status.clone();
                    let failure_output = output.clone();
                    let poll_namespace = namespace.clone();
                    let poll_globals = globals.clone();
                    let result = cljrs_async::fiber::Fiber::new(move || {
                        let _interrupt = InterruptGuard::install(interrupt.clone());
                        if interrupt.is_set() {
                            status.set("interrupted");
                            return Ok(Value::Nil);
                        }
                        let mut env = Env::new(globals, &namespace.borrow());
                        let forms =
                            cljrs_reader::Parser::new(source, "<internal-repl>".into()).parse_all();
                        let forms = match forms {
                            Ok(forms) => forms,
                            Err(error) => {
                                let _ = writeln!(output.borrow_mut(), "{error}");
                                status.set("error");
                                return Ok(Value::Nil);
                            }
                        };
                        let sink = output.clone();
                        cljrs_runtime::builtins::builtins::push_streaming_output_capture(Box::new(
                            move |s| {
                                let _ = sink.borrow_mut().write_str(s);
                            },
                        ));
                        // Fiber cancellation unwinds this guard on its own stack.
                        struct Capture;
                        impl Drop for Capture {
                            fn drop(&mut self) {
                                let _ = cljrs_runtime::builtins::builtins::pop_output_capture();
                            }
                        }
                        let _capture = Capture;
                        status.set("done");
                        for form in forms {
                            if interrupt.is_set() {
                                status.set("interrupted");
                                break;
                            }
                            let _allocation = cljrs_gc::push_alloc_frame();
                            let result = eval(&form, &mut env).and_then(|value| {
                                let _root = cljrs_runtime::env::gc_roots::root_value(&value);
                                cljrs_value::parking::park(cljrs_async::await_value(value.clone()))
                            });
                            *namespace.borrow_mut() = env.current_ns.to_string();
                            match result {
                                Ok(value) => {
                                    let _ = writeln!(output.borrow_mut(), "{value}");
                                }
                                Err(_) if interrupt.is_set() => {
                                    status.set("interrupted");
                                    break;
                                }
                                Err(error) => {
                                    let _ = writeln!(
                                        output.borrow_mut(),
                                        "{}:{}: {error}",
                                        form.span.file,
                                        form.span.line
                                    );
                                    status.set("error");
                                    break;
                                }
                            }
                        }
                        Ok(Value::Nil)
                    });
                    let result = match result {
                        Ok(fiber) => {
                            let mut task = Box::pin(fiber.into_task());
                            std::future::poll_fn(|cx| {
                                let _namespace =
                                    NamespaceScope::enter(&poll_globals, poll_namespace.clone());
                                task.as_mut().poll(cx)
                            })
                            .await
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = result {
                        failure_status.set(if cancelled.is_set() {
                            "interrupted"
                        } else {
                            "error"
                        });
                        let _ = writeln!(failure_output.borrow_mut(), "{error}");
                    }
                    // Mark terminal only after all output and fiber cleanup have finished.
                    let mut state = queue.borrow_mut();
                    if let Some(job) = state.jobs.get_mut(&id) {
                        if job.closed {
                            state.jobs.remove(&id);
                        } else {
                            job.finished = true;
                        }
                    }
                    finished.notify_one();
                });
            }
        }
    });
    for (name, arity) in [
        ("evaluation-start", 3),
        ("evaluation-poll", 1),
        ("evaluation-interrupt", 1),
        ("evaluation-close", 1),
    ] {
        let state = state.clone();
        let wake = wake.clone();
        let native = NativeFn::with_closure(
            format!("bulwark.native/{name}"),
            Arity::Fixed(arity),
            move |args| {
                use crate::native::{keyword, map};
                let mut state = state.borrow_mut();
                let identifier = id(&args[0])?;
                match name {
                    "evaluation-start" => {
                        let source = text(&args[1])?;
                        let namespace = text(&args[2])?;
                        if source.len() > 4096 || namespace.len() > 256 {
                            return Err(ValueError::Other(
                                "Evaluation source or namespace exceeds its limit".into(),
                            ));
                        }
                        if state.jobs.len() >= MAX_JOBS
                            || state
                                .jobs
                                .values()
                                .filter(|j| j.owner == identifier)
                                .count()
                                >= MAX_OWNER_JOBS
                        {
                            return Err(ValueError::Other("Evaluation capacity exhausted".into()));
                        }
                        let id = bulwark_engine::allocate_id()
                            .map_err(|e| ValueError::Other(e.to_string()))?;
                        let namespace = state
                            .jobs
                            .values()
                            .find(|job| job.owner == identifier)
                            .map(|job| job.namespace.clone())
                            .unwrap_or_else(|| Rc::new(RefCell::new(namespace)));
                        state.jobs.insert(
                            id,
                            Job {
                                owner: identifier,
                                source: Some(source),
                                namespace,
                                output: Rc::default(),
                                status: Rc::new(Cell::new("queued")),
                                interrupt: Arc::default(),
                                closed: false,
                                finished: false,
                            },
                        );
                        state.queue.push_back(id);
                        wake.notify_one();
                        Ok(Value::Long(id as i64))
                    }
                    "evaluation-close" => {
                        let ids: Vec<_> = state
                            .jobs
                            .iter()
                            .filter_map(|(id, job)| (job.owner == identifier).then_some(*id))
                            .collect();
                        for id in ids {
                            let job = state.jobs.get_mut(&id).expect("Owned job");
                            job.interrupt.request();
                            job.closed = true;
                            if job.status.get() == "queued" || job.finished {
                                state.jobs.remove(&id);
                            }
                            state.queue.retain(|queued| *queued != id);
                        }
                        Ok(Value::Nil)
                    }
                    "evaluation-interrupt" => {
                        let job = state
                            .jobs
                            .get_mut(&identifier)
                            .filter(|j| !j.closed)
                            .ok_or_else(stale)?;
                        job.interrupt.request();
                        if job.status.get() == "queued" {
                            job.source.take();
                            job.status.set("interrupted");
                            job.finished = true;
                            state.queue.retain(|queued| *queued != identifier);
                        }
                        Ok(Value::Nil)
                    }
                    _ => {
                        let job = state
                            .jobs
                            .get(&identifier)
                            .filter(|j| !j.closed)
                            .ok_or_else(stale)?;
                        let complete = job.finished;
                        let mut output = job.output.borrow_mut();
                        let result = map(vec![
                            (
                                "status",
                                keyword(if complete {
                                    job.status.get()
                                } else if job.status.get() == "queued" {
                                    "queued"
                                } else {
                                    "running"
                                }),
                            ),
                            ("output", Value::string(std::mem::take(&mut output.text))),
                            (
                                "truncated",
                                Value::Bool(std::mem::take(&mut output.truncated)),
                            ),
                            ("namespace", Value::string(job.namespace.borrow().clone())),
                        ]);
                        drop(output);
                        if complete {
                            state.jobs.remove(&identifier);
                        }
                        Ok(result)
                    }
                }
            },
        );
        globals.intern(
            "bulwark.native",
            name.into(),
            Value::NativeFunction(cljrs_gc::GcPtr::new(native)),
        );
    }
}
