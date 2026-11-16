// registry.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! One queue serializes append, observer fan-out, then handler delivery.
//! Reentrant emissions enqueue behind the current entry; callbacks never hold queue locks.

use super::{Empty, EntityId, Error, Fault, IdGeneration, Log, LogRecord, LogSnapshot, Power};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::{
    any::{TypeId, type_name},
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
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
}

impl RegistryBuilder {
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
    pub fn build(self) -> Result<Registry, StartupError> {
        let registered: HashSet<_> = self.handlers.values().map(|v| v.identity).collect();
        if !self.required.is_subset(&registered) {
            return Err(StartupError(
                "required typed action handler is missing".into(),
            ));
        }
        let error_subject = IdGeneration::next_id::<Log::Session::Query>(&Empty {})
            .map_err(|fault| StartupError(fault.message))?;
        let registry = Registry(Arc::new(Mutex::new(Inner {
            handlers: self.handlers,
            observers: Vec::new(),
            queue: VecDeque::new(),
            log: Vec::new(),
            counters: HashMap::new(),
            draining: false,
            error_chain: Chain {
                subject: error_subject,
                action: 20001,
                instance: 10001,
            },
        })));
        Ok(registry)
    }
}

struct Inner {
    handlers: HashMap<String, Registration>,
    observers: Vec<Listener>,
    queue: VecDeque<(KvEnvelope, Chain, bool)>,
    log: Vec<KvEnvelope>,
    counters: HashMap<String, u32>,
    draining: bool,
    error_chain: Chain,
}

/// Cloneable emission/observation port; it exposes no runtime-state getter.
#[derive(Clone)]
pub struct Registry(Arc<Mutex<Inner>>);

impl Registry {
    /// Register a native/FFI sink before emitting its first command.
    pub fn observe(&self, sink: Arc<dyn ObservationSink>) {
        self.0
            .lock()
            .expect("Actions queue poisoned")
            .observers
            .push(Arc::new(move |envelope, _| {
                sink.observed(envelope.clone());
                Ok(())
            }));
    }

    /// Observe typed payloads. Reentrant emissions use the same queued dispatch path.
    pub fn observe_action<A: Action>(
        &self,
        observer: impl Fn(A::Payload, &Emitter) + Send + Sync + 'static,
    ) {
        self.0
            .lock()
            .expect("Actions queue poisoned")
            .observers
            .push(Arc::new(move |envelope, emitter| {
                if let Some(payload) = envelope.payload::<A>()? {
                    observer(payload, emitter);
                }
                Ok(())
            }));
    }

    /// Fire-and-forget typed dispatch; errors themselves enter the logged observe stream.
    pub fn emit<A: Action>(&self, payload: A::Payload) {
        match self.root_chain::<A>(&payload) {
            Ok(chain) => self.enqueue::<A>(payload, chain),
            Err(fault) => self.root_fault(fault),
        }
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

    fn admit(&self, envelope: KvEnvelope) -> Result<(), Fault> {
        envelope.prefix()?;
        let value = envelope.0["_genus"].as_str().expect("validated above");
        let parts: Vec<_> = value.rsplitn(4, ':').collect();
        let chain = Chain {
            subject: EntityId::try_from(parts[2].to_owned()).expect("validated above"),
            action: parts[1].parse().expect("validated above"),
            instance: parts[0].parse().expect("validated above"),
        };
        macro_rules! admit_control {
            ($action:ty) => {
                if let Some(payload) = envelope.payload::<$action>()? {
                    self.enqueue::<$action>(payload, chain);
                    return Ok(());
                }
            };
        }
        admit_control!(Power::Session::TurnOn);
        admit_control!(Power::Session::TurnOff);
        admit_control!(Power::Session::Query);
        admit_control!(Power::Session::Quit);
        admit_control!(Log::Session::Query);
        let mut fault = Fault::new(
            "inadmissible_action",
            "This seam accepts controls and native log queries, not native outcomes.",
            true,
        );
        fault.failed_genus = Some(value.to_owned());
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
        })
    }

    fn root_fault(&self, fault: Fault) {
        // Reserve error correlation at startup so an entropy fault can still enter the bus.
        let chain = {
            let mut inner = self.0.lock().expect("Actions queue poisoned");
            inner.error_chain.instance += 1;
            inner.error_chain.clone()
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
                drop(inner);
                self.drain();
            }
            Err(fault) => self.root_fault(fault),
        }
    }

    fn drain(&self) {
        loop {
            let (envelope, emitter, observers, handler, missing, log_view) = {
                let mut inner = self.0.lock().expect("Actions queue poisoned");
                let Some((envelope, chain, command)) = inner.queue.pop_front() else {
                    inner.draining = false;
                    return;
                };
                inner.log.push(envelope.clone());
                let handler = inner
                    .handlers
                    .get(envelope.prefix().expect("internally encoded envelope"))
                    .map(|r| r.listener.clone());
                let is_log_query = envelope.prefix().expect("internally encoded envelope")
                    == route::<Log::Session::Query>();
                let missing = command && handler.is_none() && !is_log_query;
                let log_view = is_log_query.then(|| LogSnapshot {
                    entries: inner
                        .log
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| LogRecord {
                            sequence: u64::try_from(index).expect("log exceeds addressable memory"),
                            display: entry.0["_genus"]
                                .as_str()
                                .expect("internally encoded genus")
                                .to_owned(),
                        })
                        .collect(),
                });
                let emitter = Emitter {
                    registry: self.clone(),
                    origin: envelope.clone(),
                    chain: Arc::new(Mutex::new(chain)),
                };
                (
                    envelope,
                    emitter,
                    inner.observers.clone(),
                    handler,
                    missing,
                    log_view,
                )
            };
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
            if let Some(view) = log_view {
                emitter.emit::<Log::Session::Snapshot>(view);
            }
        }
    }
}

/// Opaque continuation supplied to handlers/native callbacks; no route or state is exposed.
#[derive(Clone)]
pub struct Emitter {
    registry: Registry,
    origin: KvEnvelope,
    chain: Arc<Mutex<Chain>>,
}

impl Emitter {
    /// Continue the same chain through append and fan-out, including synchronous callbacks.
    pub fn emit<A: Action>(&self, payload: A::Payload) {
        let chain = {
            let mut chain = self.chain.lock().expect("Actions continuation poisoned");
            chain.instance = chain
                .instance
                .checked_add(1)
                .expect("Actions instance range exhausted");
            chain.clone()
        };
        self.registry.enqueue::<A>(payload, chain);
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
// end registry.rs
