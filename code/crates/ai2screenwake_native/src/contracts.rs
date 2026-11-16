// contracts.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! OS-facing callbacks. Resource IDs stay in the concrete native implementation.

use ai2screenwake_backend::contract::{
    Apply, Emitter, IdleRequest, KvEnvelope, ObservationSink, Power, Registry, RegistryBuilder,
    StartupError,
};
use std::sync::Arc;

/// An initially resource-free native owner, explicitly injected at runtime startup.
///
/// Implementations must not report success until both acquisition/release outcomes
/// are known. Retain failed-release IDs; return completions through the supplied
/// continuation, including when a platform callback fires synchronously.
pub trait NativePower: Send + Sync {
    /// Acquire both resources or release all retained resources; emit Assertions::Observed.
    fn apply(&self, request: Apply, events: Emitter);
    /// Sample genuine OS idle age with paired clocks; emit Activity::Observed or its fault.
    fn sample(&self, request: IdleRequest, events: Emitter);
}

/// Native host port with one emit shape and one observation shape.
///
/// Typed Rust callers use Registry directly. Binding generation and Swift host
/// conformance are later native work, not claimed by this Rust contract.
#[derive(Clone)]
pub struct NativeSeam {
    registry: Registry,
}

impl NativeSeam {
    /// Attach the same owner bus used by all other clients.
    pub fn attach(registry: Registry) -> Self {
        Self { registry }
    }

    /// Emit a control envelope without returning core state.
    pub fn emit(&self, envelope: KvEnvelope) {
        self.registry.emit_envelope(envelope);
    }

    /// Register interest before emitting; snapshots arrive through observed().
    pub fn observe(&self, sink: Arc<dyn ObservationSink>) {
        self.registry.observe(sink);
    }
}

/// Register the exact native pair before the owner admits controls.
pub fn register(
    builder: &mut RegistryBuilder,
    native: Arc<dyn NativePower>,
) -> Result<(), StartupError> {
    builder.require::<Power::Assertions::Apply>();
    builder.require::<Power::Activity::Sample>();
    let assertions = native.clone();
    builder.on::<Power::Assertions::Apply>(move |request, events| {
        assertions.apply(request, events.clone());
        Ok(())
    })?;
    builder.on::<Power::Activity::Sample>(move |request, events| {
        native.sample(request, events.clone());
        Ok(())
    })
}
// end contracts.rs
