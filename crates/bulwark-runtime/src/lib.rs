//! Single-thread Clojurust host for Phase 0. No browser policy belongs here.

use anyhow::{Context, Result, anyhow};
use cljrs_runtime::env::{apply::apply_value, gc_roots};
use cljrs_runtime::tiered::{Env, eval};
use cljrs_runtime::{ExecutionMode, Runtime};
use cljrs_value::Value;

/// Owns one runtime environment on its creating thread.
pub struct RuntimeHost {
    env: Env,
}

impl RuntimeHost {
    /// Builds a runtime with the embedded Bulwark bootstrap namespace.
    pub fn new(mode: ExecutionMode) -> Result<Self> {
        let runtime = Runtime::builder()
            .execution_mode(mode)
            .builtin_source(
                "bulwark.core",
                include_str!("../../../clj/bulwark/core.cljrs"),
            )
            .build()
            .map_err(|error| anyhow!("{error}"))?;
        cljrs_stdlib::install(&runtime);
        let mut host = Self {
            env: runtime.env("user"),
        };
        host.eval("(require 'bulwark.core)")?;
        Ok(host)
    }

    /// Evaluates source and returns its final value. Keep retained values rooted across calls.
    pub fn eval(&mut self, source: &str) -> Result<Value> {
        let forms = cljrs_reader::Parser::new(source.into(), "<bulwark-host>".into())
            .parse_all()
            .map_err(|error| anyhow!("{error:?}"))?;
        let mut result = Value::Nil;
        for form in forms {
            let _frame = cljrs_gc::push_alloc_frame();
            result = eval(&form, &mut self.env).map_err(|error| anyhow!("{error}"))?;
        }
        Ok(result)
    }

    /// Calls the current var value, so REPL redefinitions take effect immediately.
    ///
    /// `apply_value` roots its callee and arguments across its GC safepoint.
    /// The returned value needs a host root if retained across another evaluation.
    pub fn call(&mut self, namespace: &str, name: &str, args: Vec<Value>) -> Result<Value> {
        let function = self
            .env
            .globals
            .lookup_in_ns(namespace, name)
            .with_context(|| format!("Unbound function: {namespace}/{name}"))?;
        apply_value(&function, args, &mut self.env).map_err(|error| anyhow!("{error}"))
    }

    /// Collects unreachable values at an explicit host safepoint.
    pub fn collect(&self) {
        gc_roots::force_collect(&self.env);
    }

    /// Exposes the environment for Phase 0 executor and GC experiments.
    pub fn env(&self) -> &Env {
        &self.env
    }
}
