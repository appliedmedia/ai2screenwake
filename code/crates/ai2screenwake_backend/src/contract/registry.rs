// registry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! One queue serializes append, observer fan-out, then handler delivery.
//! Reentrant emissions enqueue behind the current entry; callbacks never hold queue locks.
//!
//! ```
//! use ai2screenwake_backend::contract::{
//!     Empty, Power, RegistryBuilder, StartupError, Status,
//! };
//! let mut builder = RegistryBuilder::default();
//! builder.on::<Power::Session::Query>(|_, events| {
//!     events.emit::<Power::Session::Snapshot>(Status::off());
//!     Ok(())
//! })?;
//! let bus = builder.build()?;
//! bus.observe_action::<Power::Session::Snapshot>(|snapshot, _| {
//!     assert_eq!(snapshot.is_on, Some(false));
//! });
//! bus.emit::<Power::Session::Query>(Empty {});
//! # Ok::<(), StartupError>(())
//! ```

use super::{
    ActionJournal, Empty, EntityId, Error, Fault, IdGeneration, JournalEntry, LOG_MEMORY_LIMIT,
    Log, LogRecord, LogSnapshot, Power, RejectedApply,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::{
    any::{TypeId, type_name},
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    marker::PhantomData,
    sync::{Arc, Condvar, Mutex, Weak},
    thread,
};

pub(super) mod sealed {
    pub trait Sealed {}
}

/// Closed semantic action identity. Route keys remain private to this framework module.
pub trait Action: sealed::Sealed + Send + Sync + 'static {
    /// Canonical typed KV payload.
    type Payload: Clone + DeserializeOwned + Serialize + Send + Sync + 'static;
    /// Commands require a registered handler; events may be observation-only.
    const COMMAND: bool;
}

pub(super) fn route<A: Action>() -> String {
    let path = type_name::<A>();
    let (_, hierarchy) = path
        .split_once("::contract::")
        .expect("sealed action must belong to the contract hierarchy");
    debug_assert!((2..=4).contains(&hierarchy.split("::").count()));
    hierarchy.replace("::", ":")
}

/// The single serializable KV wire shape. Routing and correlation are framework-owned.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct KvEnvelope(Map<String, Value>);

impl KvEnvelope {
    /// Validate stored wire metadata without admitting or redispatching historical work.
    pub fn validate_history(&self) -> Result<(), Fault> {
        self.prefix().map(|_| ())
    }

    /// Attribute diagnostics to a known action type without exposing its route key.
    pub fn attributed<A: Action>(&self, mut fault: Fault) -> Fault {
        if self.prefix().is_ok_and(|prefix| prefix == route::<A>()) {
            fault.failed_genus = self
                .0
                .get("_genus")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        fault
    }

    /// Extract an opaque request token without exposing any route component.
    pub fn correlation(&self) -> Result<CorrelationId, Fault> {
        self.prefix()?;
        let value = self.0["_genus"].as_str().expect("validated above");
        let parts: Vec<_> = value.rsplitn(4, ':').collect();
        Ok(CorrelationId {
            subject: EntityId::try_from(parts[2].to_owned()).expect("validated above"),
            action: parts[1].parse().expect("validated above"),
        })
    }

    /// Decode only a named typed action, without exposing routing keys to consumers.
    pub fn payload<A: Action>(&self) -> Result<Option<A::Payload>, Fault> {
        if self.prefix()? != route::<A>() {
            return Ok(None);
        }
        let mut payload = self.0.clone();
        payload.remove("_genus");
        serde_json::from_value(Value::Object(payload))
            .map(Some)
            .map_err(|_| {
                Fault::new(
                    "invalid_payload",
                    "Action payload does not match its contract.",
                    true,
                )
            })
    }

    fn prefix(&self) -> Result<&str, Fault> {
        let value = self
            .0
            .get("_genus")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Fault::new("invalid_envelope", "Action correlation is missing.", true)
            })?;
        let mut parts = value.rsplitn(4, ':');
        let instance = parts.next().and_then(|v| v.parse::<u64>().ok());
        let action = parts.next().and_then(|v| v.parse::<u32>().ok());
        let subject = parts
            .next()
            .and_then(|v| EntityId::try_from(v.to_owned()).ok());
        let prefix = parts.next();
        match (prefix, subject, action, instance) {
            (Some(prefix), Some(_), Some(20001..=99999), Some(10001..))
                if (2..=4).contains(&prefix.split(':').count()) =>
            {
                Ok(prefix)
            }
            _ => Err(Fault::new(
                "invalid_envelope",
                "Action correlation is malformed.",
                true,
            )),
        }
    }

    fn make<A: Action>(payload: &A::Payload, chain: &Chain) -> Result<Self, Fault> {
        let Value::Object(mut map) = serde_json::to_value(payload)
            .map_err(|_| Fault::new("invalid_payload", "Cannot encode action payload.", true))?
        else {
            return Err(Fault::new(
                "invalid_payload",
                "Action payload must be a KV object.",
                true,
            ));
        };
        map.insert(
            "_genus".into(),
            Value::String(format!(
                "{}:{}:{}:{}",
                route::<A>(),
                chain.subject,
                chain.action,
                chain.instance,
            )),
        );
        Ok(Self(map))
    }
}

/// Native/FFI observer. Outcomes are delivered here, never returned from emit.
pub trait ObservationSink: Send + Sync {
    /// Fold/render a logged envelope; do not throw or perform blocking UI work.
    fn observed(&self, envelope: KvEnvelope);
}

/// Opaque request identity shared by every continuation in a chain; never a route.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorrelationId {
    subject: EntityId,
    action: u32,
}

/// Prepared metadata allows interest registration before an intent is emitted.
/// This value contains no core response or state and cannot be cloned/replayed.
pub struct PreparedAction<A: Action> {
    payload: A::Payload,
    chain: Chain,
    identity: PhantomData<A>,
}

impl<A: Action> PreparedAction<A> {
    /// Obtain the future request token before registering the result observer.
    pub fn correlation(&self) -> CorrelationId {
        CorrelationId {
            subject: self.chain.subject.clone(),
            action: self.chain.action,
        }
    }
}

/// Explicit startup failure before an operational bus exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupError(pub String);

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for StartupError {}

#[derive(Clone)]
struct Chain {
    subject: EntityId,
    action: u32,
    instance: u64,
    next_instance: Arc<Mutex<u64>>,
}

impl Chain {
    fn continued(&self) -> Self {
        let mut next = self
            .next_instance
            .lock()
            .expect("Actions continuation poisoned");
        *next = next
            .checked_add(1)
            .expect("Actions instance range exhausted");
        Self {
            instance: *next,
            ..self.clone()
        }
    }
}

type Listener = Arc<dyn Fn(&KvEnvelope, &Emitter) -> Result<(), Fault> + Send + Sync>;

struct Registration {
    identity: TypeId,
    listener: Listener,
}

/// Build all required handlers before admitting commands; no service locator is exposed.
#[derive(Default)]
pub struct RegistryBuilder {
    handlers: HashMap<String, Registration>,
    required: HashSet<TypeId>,
    journal: Option<Arc<dyn ActionJournal>>,
}

#[derive(Default)]
struct DispatchState {
    ready: bool,
    stopping: bool,
}

#[derive(Default)]
struct DispatchSignal {
    state: Mutex<DispatchState>,
    changed: Condvar,
}

fn persistent_dispatch(registry: Weak<Mutex<Inner>>, signal: Arc<DispatchSignal>) {
    loop {
        let mut state = signal.state.lock().expect("dispatch signal poisoned");
        while !state.ready && !state.stopping {
            state = signal
                .changed
                .wait(state)
                .expect("dispatch signal poisoned");
        }
        if state.stopping {
            return;
        }
        state.ready = false;
        drop(state);
        let Some(registry) = registry.upgrade() else {
            return;
        };
        Registry(registry).drain();
    }
}

impl RegistryBuilder {
    /// Configure the first append observer; absent means an explicitly ephemeral host/test bus.
    pub fn journal(&mut self, journal: Arc<dyn ActionJournal>) {
        self.journal = Some(journal);
    }

    /// Require a typed collaborator at build time.
    pub fn require<A: Action>(&mut self) {
        self.required.insert(TypeId::of::<A>());
    }

    /// Register exactly one handler for a typed action.
    pub fn on<A: Action>(
        &mut self,
        handler: impl Fn(A::Payload, &Emitter) -> Result<(), Fault> + Send + Sync + 'static,
    ) -> Result<(), StartupError> {
        if self.handlers.contains_key(&route::<A>()) {
            return Err(StartupError("duplicate typed action registration".into()));
        }
        self.handlers.insert(
            route::<A>(),
            Registration {
                identity: TypeId::of::<A>(),
                listener: Arc::new(move |envelope, emitter| {
                    let payload = envelope.payload::<A>()?.ok_or_else(|| {
                        Fault::new(
                            "wrong_action",
                            "Registered action identity mismatch.",
                            false,
                        )
                    })?;
                    handler(payload, emitter)
                }),
            },
        );
        Ok(())
    }

    /// Reject missing required handlers before the runtime can emit any commands.
    pub fn build(mut self) -> Result<Registry, StartupError> {
        self.on::<Log::Session::Query>(|_, events| {
            let view = {
                let inner = events.registry.0.lock().expect("Actions queue poisoned");
                LogSnapshot {
                    entries: inner
                        .log
                        .iter()
                        .map(|entry| LogRecord {
                            sequence: entry.sequence,
                            display: entry.envelope.0["_genus"]
                                .as_str()
                                .expect("internally encoded genus")
                                .to_owned(),
                        })
                        .collect(),
                    earlier_entries_omitted: inner
                        .log
                        .front()
                        .is_some_and(|entry| entry.sequence > 0),
                    storage_error: inner.journal_fault.clone(),
                }
            };
            events.emit::<Log::Session::Snapshot>(view);
            Ok(())
        })?;
        let registered: HashSet<_> = self.handlers.values().map(|v| v.identity).collect();
        if !self.required.is_subset(&registered) {
            return Err(StartupError(
                "required typed action handler is missing".into(),
            ));
        }
        let error_subject = IdGeneration::next_id::<Log::Session::Query>(&Empty {})
            .map_err(|fault| StartupError(fault.message))?;
        let history = match &self.journal {
            Some(journal) => journal.history().map_err(|fault| {
                StartupError(format!("{}: {}", fault.fault_code, fault.message))
            })?,
            None => Default::default(),
        };
        if history.entries.len() > LOG_MEMORY_LIMIT
            || history
                .entries
                .windows(2)
                .any(|entries| entries[0].sequence.checked_add(1) != Some(entries[1].sequence))
            || history
                .entries
                .last()
                .is_some_and(|last| last.sequence.checked_add(1) != Some(history.next_sequence))
            || (history.entries.is_empty() && history.next_sequence != 0)
        {
            return Err(StartupError(
                "journal_history: invalid bounded history sequence".into(),
            ));
        }
        for entry in &history.entries {
            entry
                .envelope
                .validate_history()
                .map_err(|fault| StartupError(fault.message))?;
        }
        let dispatch = self
            .journal
            .as_ref()
            .map(|_| Arc::new(DispatchSignal::default()));
        let registry = Registry(Arc::new(Mutex::new(Inner {
            handlers: self.handlers,
            observers: BTreeMap::new(),
            next_observer: 0,
            queue: VecDeque::new(),
            log: history.entries.into(),
            next_sequence: history.next_sequence,
            journal: self.journal,
            journal_fault: None,
            dispatch: dispatch.clone(),
            counters: HashMap::new(),
            draining: false,
            error_chain: Chain {
                subject: error_subject,
                action: 20001,
                instance: 10001,
                next_instance: Arc::new(Mutex::new(10001)),
            },
        })));
        if let Some(signal) = dispatch {
            let weak = Arc::downgrade(&registry.0);
            thread::Builder::new()
                .name("screenwake-actions".into())
                .spawn(move || persistent_dispatch(weak, signal))
                .map_err(|error| StartupError(error.to_string()))?;
        }
        Ok(registry)
    }
}

struct Inner {
    handlers: HashMap<String, Registration>,
    observers: BTreeMap<u64, Listener>,
    next_observer: u64,
    queue: VecDeque<(KvEnvelope, Chain, bool)>,
    log: VecDeque<JournalEntry>,
    next_sequence: u64,
    journal: Option<Arc<dyn ActionJournal>>,
    journal_fault: Option<Fault>,
    dispatch: Option<Arc<DispatchSignal>>,
    counters: HashMap<String, u32>,
    draining: bool,
    error_chain: Chain,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(signal) = &self.dispatch {
            let mut state = signal.state.lock().expect("dispatch signal poisoned");
            state.stopping = true;
            signal.changed.notify_one();
        }
    }
}

/// Cloneable emission/observation port; it exposes no runtime-state getter.
#[derive(Clone)]
pub struct Registry(Arc<Mutex<Inner>>);

/// Scoped observation; dropping it unregisters future peer callbacks.
pub struct Subscription {
    registry: Weak<Mutex<Inner>>,
    identity: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry
                .lock()
                .expect("Actions queue poisoned")
                .observers
                .remove(&self.identity);
        }
    }
}

impl Registry {
    fn add_observer(&self, listener: Listener) -> u64 {
        let mut inner = self.0.lock().expect("Actions queue poisoned");
        inner.next_observer += 1;
        let identity = inner.next_observer;
        inner.observers.insert(identity, listener);
        identity
    }

    /// Register a native/FFI sink before emitting its first command.
    pub fn observe(&self, sink: Arc<dyn ObservationSink>) {
        self.add_observer(Arc::new(move |envelope, _| {
            sink.observed(envelope.clone());
            Ok(())
        }));
    }

    /// Register a peer-lifetime observer instead of retaining one callback per connection.
    pub fn subscribe(&self, sink: Arc<dyn ObservationSink>) -> Subscription {
        let identity = self.add_observer(Arc::new(move |envelope, _| {
            sink.observed(envelope.clone());
            Ok(())
        }));
        Subscription {
            registry: Arc::downgrade(&self.0),
            identity,
        }
    }

    /// Observe typed payloads. Reentrant emissions use the same queued dispatch path.
    pub fn observe_action<A: Action>(
        &self,
        observer: impl Fn(A::Payload, &Emitter) + Send + Sync + 'static,
    ) {
        self.add_observer(Arc::new(move |envelope, emitter| {
            if let Some(payload) = envelope.payload::<A>()? {
                observer(payload, emitter);
            }
            Ok(())
        }));
    }

    /// Fire-and-forget typed dispatch; errors themselves enter the logged observe stream.
    pub fn emit<A: Action>(&self, payload: A::Payload) {
        if let Ok(prepared) = self.prepare::<A>(payload) {
            self.emit_prepared(prepared);
        }
    }

    /// Reserve correlation metadata only; errors dispatch before being returned.
    pub fn prepare<A: Action>(&self, payload: A::Payload) -> Result<PreparedAction<A>, Fault> {
        match self.root_chain::<A>(&payload) {
            Ok(chain) => Ok(PreparedAction {
                payload,
                chain,
                identity: PhantomData,
            }),
            Err(fault) => {
                self.root_fault(fault.clone());
                Err(fault)
            }
        }
    }

    /// Emit a prepared request after its result observer has been registered.
    pub fn emit_prepared<A: Action>(&self, prepared: PreparedAction<A>) {
        self.enqueue::<A>(prepared.payload, prepared.chain);
    }

    /// Admit a native UI envelope, never a forged native completion or projected state.
    ///
    /// The future IPC adapter must additionally enforce its narrower three-control
    /// allowlist and same-user peer checks before entering this in-process seam.
    pub fn emit_envelope(&self, envelope: KvEnvelope) {
        if let Err(fault) = self.admit(envelope) {
            self.root_fault(fault);
        }
    }

    /// Receive authenticated owner observations; this cannot admit a remote command.
    pub fn receive_owner(&self, envelope: KvEnvelope) {
        let result = (|| {
            envelope.prefix()?;
            let value = envelope.0["_genus"].as_str().expect("validated above");
            let parts: Vec<_> = value.rsplitn(4, ':').collect();
            let chain = Chain {
                subject: EntityId::try_from(parts[2].to_owned()).expect("validated above"),
                action: parts[1].parse().expect("validated above"),
                instance: parts[0].parse().expect("validated above"),
                next_instance: Arc::new(Mutex::new(parts[0].parse().expect("validated above"))),
            };
            macro_rules! receive {
                ($action:ty) => {
                    if let Some(payload) = envelope.payload::<$action>()? {
                        self.enqueue::<$action>(payload, chain);
                        return Ok(());
                    }
                };
            }
            receive!(Power::Session::Settled);
            receive!(Power::Session::Snapshot);
            receive!(Error::Native::Assertions::Occurred);
            receive!(Error::Native::Activity::Occurred);
            receive!(Error::Dispatch::Validation::Occurred);
            receive!(Error::Ipc::Protocol::Occurred);
            receive!(Error::Log::Storage::Occurred);
            Err(Fault::new(
                "inadmissible_observation",
                "The owner sent an invalid observation.",
                true,
            ))
        })();
        if let Err(fault) = result {
            self.root_fault(fault);
        }
    }

    fn admit(&self, envelope: KvEnvelope) -> Result<(), Fault> {
        envelope.prefix()?;
        let value = envelope.0["_genus"].as_str().expect("validated above");
        let parts: Vec<_> = value.rsplitn(4, ':').collect();
        let chain = Chain {
            subject: EntityId::try_from(parts[2].to_owned()).expect("validated above"),
            action: parts[1].parse().expect("validated above"),
            instance: parts[0].parse().expect("validated above"),
            next_instance: Arc::new(Mutex::new(parts[0].parse().expect("validated above"))),
        };
        macro_rules! admit_control {
            ($action:ty) => {
                if let Some(payload) = envelope.payload::<$action>().map_err(|mut fault| {
                    fault.failed_genus = Some(value.to_owned());
                    fault
                })? {
                    self.enqueue::<$action>(payload, chain);
                    return Ok(());
                }
            };
        }
        admit_control!(Power::Session::TurnOn);
        admit_control!(Power::Session::TurnOff);
        admit_control!(Power::Session::Toggle);
        admit_control!(Power::Session::Query);
        admit_control!(Power::Session::Quit);
        admit_control!(Log::Session::Query);
        let mut fault = Fault::new(
            "inadmissible_action",
            "This seam accepts controls and native log queries, not native outcomes.",
            true,
        );
        if self
            .0
            .lock()
            .expect("Actions queue poisoned")
            .handlers
            .contains_key(envelope.prefix()?)
        {
            fault.failed_genus = Some(value.to_owned());
        }
        Err(fault)
    }

    fn root_chain<A: Action>(&self, payload: &A::Payload) -> Result<Chain, Fault> {
        let subject = IdGeneration::next_id::<A>(payload)?;
        let mut inner = self.0.lock().expect("Actions queue poisoned");
        let counter = inner.counters.entry(route::<A>()).or_insert(20000);
        if *counter == 99999 {
            return Err(Fault::new(
                "action_id_exhausted",
                "Action identity range is exhausted.",
                false,
            ));
        }
        *counter += 1;
        Ok(Chain {
            subject,
            action: *counter,
            instance: 10001,
            next_instance: Arc::new(Mutex::new(10001)),
        })
    }

    fn root_fault(&self, fault: Fault) {
        // Reserve error correlation at startup so an entropy fault can still enter the bus.
        let chain = {
            let inner = self.0.lock().expect("Actions queue poisoned");
            inner.error_chain.continued()
        };
        self.enqueue::<Error::Dispatch::Validation::Occurred>(fault, chain);
    }

    fn enqueue<A: Action>(&self, payload: A::Payload, chain: Chain) {
        match KvEnvelope::make::<A>(&payload, &chain) {
            Ok(envelope) => {
                let mut inner = self.0.lock().expect("Actions queue poisoned");
                inner.queue.push_back((envelope, chain, A::COMMAND));
                if inner.draining {
                    return;
                }
                inner.draining = true;
                let dispatch = inner.dispatch.clone();
                drop(inner);
                if let Some(signal) = dispatch {
                    let mut state = signal.state.lock().expect("dispatch signal poisoned");
                    state.ready = true;
                    signal.changed.notify_one();
                } else {
                    self.drain();
                }
            }
            Err(fault) => self.root_fault(fault),
        }
    }

    fn drain(&self) {
        loop {
            let (envelope, chain, command, sequence, journal, prior_fault) = {
                let mut inner = self.0.lock().expect("Actions queue poisoned");
                let Some((envelope, chain, command)) = inner.queue.pop_front() else {
                    inner.draining = false;
                    return;
                };
                let sequence = inner.next_sequence;
                inner.next_sequence = sequence.checked_add(1).expect("journal sequence exhausted");
                (
                    envelope,
                    chain,
                    command,
                    sequence,
                    inner.journal.clone(),
                    inner.journal_fault.clone(),
                )
            };
            let append_failure = if prior_fault.is_none() {
                journal
                    .as_ref()
                    .and_then(|journal| journal.append(sequence, &envelope).err())
            } else {
                None
            };
            let (emitter, observers, handler, missing) = {
                let mut inner = self.0.lock().expect("Actions queue poisoned");
                if let Some(mut fault) = append_failure.clone() {
                    fault.failed_genus = envelope
                        .0
                        .get("_genus")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    inner.journal_fault = Some(fault);
                }
                inner.log.push_back(JournalEntry {
                    sequence,
                    envelope: envelope.clone(),
                });
                if inner.log.len() > LOG_MEMORY_LIMIT {
                    inner.log.pop_front();
                }
                let handler = inner
                    .handlers
                    .get(envelope.prefix().expect("internally encoded envelope"))
                    .map(|r| r.listener.clone());
                let missing = command && handler.is_none();
                let emitter = Emitter {
                    registry: self.clone(),
                    origin: envelope.clone(),
                    chain,
                    persistence_error: inner.journal_fault.clone(),
                };
                (
                    emitter,
                    inner.observers.values().cloned().collect::<Vec<_>>(),
                    handler,
                    missing,
                )
            };
            if let Some(fault) = append_failure {
                emitter.error::<Error::Log::Storage::Occurred>(fault);
            }
            if let Some(fault) = emitter.persistence_error.clone() {
                if let Some(request) = envelope
                    .payload::<Power::Assertions::Apply>()
                    .expect("typed enabling action")
                {
                    if request.enabled {
                        emitter.error::<Error::Log::Storage::Occurred>(fault.clone());
                        emitter
                            .emit::<Power::Assertions::Rejected>(RejectedApply { request, fault });
                        continue;
                    }
                }
                if let Some(mut settlement) = envelope
                    .payload::<Power::Session::Settled>()
                    .expect("typed settlement")
                {
                    if settlement.result.error.is_none() {
                        emitter.error::<Error::Log::Storage::Occurred>(fault.clone());
                        settlement.result.error = Some(fault);
                        if let Some(status) = &mut settlement.result.status {
                            status.protected_idle_duration_seconds = None;
                        }
                        emitter.emit::<Power::Session::Settled>(settlement);
                        continue;
                    }
                }
                if let Some(mut view) = envelope
                    .payload::<Log::Session::Snapshot>()
                    .expect("typed log view")
                {
                    if view.storage_error.is_none() {
                        emitter.error::<Error::Log::Storage::Occurred>(fault.clone());
                        view.storage_error = Some(fault);
                        emitter.emit::<Log::Session::Snapshot>(view);
                        continue;
                    }
                }
            }
            for callback in observers.into_iter().chain(handler) {
                if let Err(fault) = callback(&envelope, &emitter) {
                    emitter.error::<Error::Dispatch::Validation::Occurred>(fault);
                }
            }
            if missing {
                emitter.error::<Error::Dispatch::Validation::Occurred>(Fault::new(
                    "unregistered_action",
                    "Command has no registered handler.",
                    false,
                ));
            }
        }
    }
}

/// Opaque continuation supplied to handlers/native callbacks; no route or state is exposed.
#[derive(Clone)]
pub struct Emitter {
    registry: Registry,
    origin: KvEnvelope,
    chain: Chain,
    persistence_error: Option<Fault>,
}

impl Emitter {
    /// Current append receipt, not a business-state query or a UI response.
    pub fn persistence_error(&self) -> Option<&Fault> {
        self.persistence_error.as_ref()
    }

    /// The current request token, without caller-visible genus parsing.
    pub fn correlation(&self) -> CorrelationId {
        CorrelationId {
            subject: self.chain.subject.clone(),
            action: self.chain.action,
        }
    }

    /// The current wire entry for a registered transport handler to forward unchanged.
    pub fn envelope(&self) -> KvEnvelope {
        self.origin.clone()
    }

    /// Do not keep a runtime alive just to retain a notification or deadline.
    pub fn downgrade(&self) -> WeakEmitter {
        WeakEmitter {
            registry: Arc::downgrade(&self.registry.0),
            origin: self.origin.clone(),
            chain: self.chain.clone(),
            persistence_error: self.persistence_error.clone(),
        }
    }

    /// Retain only weak dispatch provenance, not a queued copy of a potentially private payload.
    pub fn downgrade_metadata(&self) -> WeakEmitter {
        let mut weak = self.downgrade();
        weak.origin.0.retain(|key, _| key == "_genus");
        weak
    }

    /// Continue the same chain through append and fan-out, including synchronous callbacks.
    pub fn emit<A: Action>(&self, payload: A::Payload) {
        self.registry.enqueue::<A>(payload, self.chain.continued());
    }

    /// Dispatch an error first and preserve the original failed action for observers.
    pub fn error<A: Action<Payload = Fault>>(&self, mut fault: Fault) {
        fault.failed_genus = self
            .origin
            .0
            .get("_genus")
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.emit::<A>(fault);
    }
}

/// Non-owning continuation for request tracking, timers and native lifecycle listeners.
#[derive(Clone)]
pub struct WeakEmitter {
    registry: Weak<Mutex<Inner>>,
    origin: KvEnvelope,
    chain: Chain,
    persistence_error: Option<Fault>,
}

impl WeakEmitter {
    /// None means the owner was disposed, not a live operation that may be reported successful.
    pub fn upgrade(&self) -> Option<Emitter> {
        self.registry.upgrade().map(|registry| Emitter {
            registry: Registry(registry),
            origin: self.origin.clone(),
            chain: self.chain.clone(),
            persistence_error: self.persistence_error.clone(),
        })
    }
}
// end registry.rs
