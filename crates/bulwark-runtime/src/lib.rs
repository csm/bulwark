//! Thread-affine Clojurust host. No browser policy belongs here.

pub mod native;
pub mod nrepl;
pub mod worker;

use anyhow::{Context, Result, anyhow};
use cljrs_runtime::env::error::EvalResult;
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
        Self::build(mode, None)
    }

    /// Must run inside the owning thread's Tokio LocalSet.
    pub fn with_bridge(
        mode: ExecutionMode,
        endpoint: bulwark_engine::bridge::RuntimeEndpoint,
    ) -> Result<Self> {
        Self::build(mode, Some(endpoint))
    }

    fn build(
        mode: ExecutionMode,
        endpoint: Option<bulwark_engine::bridge::RuntimeEndpoint>,
    ) -> Result<Self> {
        let runtime = Runtime::builder()
            .execution_mode(mode)
            .builtin_source(
                "bulwark.browser",
                include_str!("../../../clj/bulwark/browser.cljrs"),
            )
            .builtin_source(
                "bulwark.commands",
                include_str!("../../../clj/bulwark/commands.cljrs"),
            )
            .builtin_source(
                "bulwark.hooks",
                include_str!("../../../clj/bulwark/hooks.cljrs"),
            )
            .builtin_source(
                "bulwark.keymap",
                include_str!("../../../clj/bulwark/keymap.cljrs"),
            )
            .builtin_source(
                "bulwark.buffer",
                include_str!("../../../clj/bulwark/buffer.cljrs"),
            )
            .builtin_source(
                "bulwark.window",
                include_str!("../../../clj/bulwark/window.cljrs"),
            )
            .builtin_source(
                "bulwark.core",
                include_str!("../../../clj/bulwark/core.cljrs"),
            )
            .builtin_source(
                "bulwark.bridge",
                include_str!("../../../clj/bulwark/bridge.cljrs"),
            )
            .builtin_source(
                "bulwark.shell",
                include_str!("../../../clj/bulwark/shell.cljrs"),
            )
            .build()
            .map_err(|error| anyhow!("{error}"))?;
        cljrs_stdlib::install(&runtime);
        let mut host = Self {
            env: runtime.env("user"),
        };
        let has_bridge = endpoint.is_some();
        if let Some(endpoint) = endpoint {
            cljrs_async::init(&host.env.globals);
            native::install(&host.env.globals, endpoint);
        }
        host.eval("(require 'bulwark.core)")?;
        if has_bridge {
            host.eval("(bulwark.keymap/set-route-publisher! bulwark.native/set-key-routes!)")?;
            host.eval(
                "(bulwark.buffer/set-host! {:create bulwark.native/webview-create \
                                           :close bulwark.native/webview-close \
                                           :select bulwark.native/webview-select \
                                           :next-id bulwark.native/allocate-id!})",
            )?;
        }
        Ok(host)
    }

    /// Evaluates source and returns its final value. Keep retained values rooted across calls.
    pub fn eval(&mut self, source: &str) -> Result<Value> {
        self.eval_with(source, eval)
    }

    /// Evaluates source with a caller-chosen form evaluator.
    ///
    /// A shell passes a root-fiber driver here so top-level forms may park.
    pub fn eval_with(
        &mut self,
        source: &str,
        mut eval_form: impl FnMut(&cljrs_reader::Form, &mut Env) -> EvalResult,
    ) -> Result<Value> {
        let forms = cljrs_reader::Parser::new(source.into(), "<bulwark-host>".into())
            .parse_all()
            .map_err(|error| anyhow!("{error:?}"))?;
        let mut result = Value::Nil;
        for form in forms {
            let _frame = cljrs_gc::push_alloc_frame();
            result = eval_form(&form, &mut self.env).map_err(|error| anyhow!("{error}"))?;
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
