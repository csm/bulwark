use anyhow::{Result, ensure};
use bulwark_runtime::RuntimeHost;
use cljrs_gc::GcPtr;
use cljrs_runtime::ExecutionMode;
use cljrs_value::{Arity, Keyword, MapValue, NativeFn, Value, ValueError, ValueResult};
use std::time::Instant;

fn main() -> Result<()> {
    for mode in [ExecutionMode::TreeWalk, ExecutionMode::Tiered] {
        run(mode)?;
    }
    Ok(())
}

// The upstream callback contract returns its unboxed EvalError.
#[allow(clippy::result_large_err)]
fn collect_for_probe(_: &[Value]) -> ValueResult<Value> {
    cljrs_runtime::env::callback::with_eval_context(|env| {
        cljrs_runtime::env::gc_roots::force_collect(env);
        Ok(Value::Nil)
    })
    .map_err(|error| ValueError::Other(error.to_string()))
}

fn run(mode: ExecutionMode) -> Result<()> {
    let mut host = RuntimeHost::new(mode)?;
    host.env().globals.intern(
        "bulwark.native",
        "collect!".into(),
        Value::NativeFunction(GcPtr::new(NativeFn::new(
            "bulwark.native/collect!",
            Arity::Fixed(0),
            collect_for_probe,
        ))),
    );
    host.env().globals.mark_loaded("bulwark.native");
    host.eval(include_str!("../dispatcher.cljrs"))?;
    let started = Instant::now();
    let mut live_counts = Vec::new();
    // Allocation frames end after each call. A single outer frame would retain
    // every temporary and make this GC experiment meaningless.
    for sequence in 1..=10_000 {
        {
            let _frame = cljrs_gc::push_alloc_frame();
            let event = Value::Map(MapValue::from_pairs(vec![
                (
                    Value::keyword(Keyword::qualified("event", "type")),
                    Value::keyword(Keyword::qualified("page", "title-changed")),
                ),
                (
                    Value::keyword(Keyword::simple("sequence")),
                    Value::Long(sequence),
                ),
                (
                    Value::keyword(Keyword::simple("title")),
                    Value::string(format!("Page {sequence}")),
                ),
            ]));
            let result = host.call("bulwark.experiment", "dispatch!", vec![event])?;
            ensure!(
                matches!(result, Value::Long(n) if n == sequence),
                "Incorrect callback result at {sequence}"
            );
        }
        if sequence % 100 == 0 {
            host.collect();
            if sequence >= 1_000 {
                live_counts.push(cljrs_gc::HEAP.count());
            }
        }
    }
    ensure!(matches!(
        host.eval("@bulwark.experiment/delivered")?,
        Value::Long(10_000)
    ));
    let error = host
        .call("bulwark.experiment", "fail!", vec![])
        .expect_err("Callback exception was lost");
    ensure!(
        error.to_string().contains("expected callback error"),
        "Unexpected callback error: {error}"
    );
    host.eval("(in-ns 'bulwark.experiment) (defn dispatch! [event] :redefined)")?;
    let result = host.call("bulwark.experiment", "dispatch!", vec![Value::Nil])?;
    ensure!(
        result.to_string() == ":redefined",
        "Host cached a stale dispatcher"
    );
    let min_live = live_counts.iter().min().unwrap();
    let max_live = live_counts.iter().max().unwrap();
    ensure!(
        max_live - min_live < 200,
        "Live objects grew after warmup: {min_live}..{max_live}"
    );
    println!(
        "E2 {mode:?}: 10,000 ordered map callbacks, 100 in-callback and 100 host collections, live objects after warmup={min_live}..{max_live}, exception propagation, live redefinition; {:?}",
        started.elapsed()
    );
    Ok(())
}
