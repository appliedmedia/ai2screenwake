// contracts.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! OS-facing callbacks. Resource IDs stay in the concrete native implementation.

use ai2screenwake_backend::contract::{
    Apply, CornerAuthorization, Emitter, IdleRequest, KvEnvelope, ObservationSink, Power, Registry,
    RegistryBuilder, StartupError,
};
use std::sync::Arc;

/// An initially resource-free native owner, explicitly injected at runtime startup.
///
/// Implementations must not report success until both acquisition/release outcomes
/// are known. Retain failed-release IDs; return completions through the supplied
/// continuation, including when a platform callback fires synchronously.
pub trait NativePower: Send + Sync {
    /// Ordered owner admission for an installed native corner source.
    fn authorize_corners(&self, _authorization: CornerAuthorization) {}
    /// Inbound Off/Toggle/Quit observation retires filtering before owner handling.
    fn retire_corners(&self) {}
    /// Bind source-health observation through Actions when this implementation owns a source.
    ///
    /// Stateless injected seams have no background source. Source-owning implementations
    /// must publish current availability and independently fence acquisition during loss.
    fn watch_lifecycle(&self, _events: Emitter) {}
    /// Acquire/release resources; emit Observed or clockless terminal Failed after error dispatch.
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

/// Register native assertion, activity, lifecycle and corner capabilities.
pub fn register(
    builder: &mut RegistryBuilder,
    native: Arc<dyn NativePower>,
) -> Result<(), StartupError> {
    builder.require::<Power::Assertions::Resources::Apply>();
    builder.require::<Power::Activity::System::Sample>();
    let corners = native.clone();
    builder.on::<Power::Corners::Guard::Authorize>(move |authorization, _| {
        corners.authorize_corners(authorization);
        Ok(())
    })?;
    let lifecycle = native.clone();
    builder.on::<Power::Activity::System::Watch>(move |_, events| {
        lifecycle.watch_lifecycle(events.clone());
        Ok(())
    })?;
    let assertions = native.clone();
    builder.on::<Power::Assertions::Resources::Apply>(move |request, events| {
        assertions.apply(request, events.clone());
        Ok(())
    })?;
    builder.on::<Power::Activity::System::Sample>(move |request, events| {
        native.sample(request, events.clone());
        Ok(())
    })
}

/// Attach input-retirement observation before the owner can receive controls.
pub fn observe_controls(registry: &Registry, native: &Arc<dyn NativePower>) {
    struct Controls(std::sync::Weak<dyn NativePower>);
    impl ObservationSink for Controls {
        fn observed(&self, envelope: KvEnvelope) {
            let retires = envelope
                .payload::<Power::Session::Protection::TurnOff>()
                .expect("canonical control envelope")
                .is_some()
                || envelope
                    .payload::<Power::Session::Protection::Toggle>()
                    .expect("canonical control envelope")
                    .is_some()
                || envelope
                    .payload::<Power::Session::Protection::Quit>()
                    .expect("canonical control envelope")
                    .is_some();
            if retires {
                if let Some(native) = self.0.upgrade() {
                    native.retire_corners();
                }
            }
        }
    }
    registry.observe(Arc::new(Controls(Arc::downgrade(native))));
}
// end contracts.rs
