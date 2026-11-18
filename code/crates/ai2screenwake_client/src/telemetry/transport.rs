// transport.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! One bounded owner queue; terminal SDK sends and all SDK teardown stay off Actions/UI threads.

use super::{Destinations, Metadata};
use crate::contract::{
    Emitter, Empty, Error, Fault, ForwardingReceipt, ForwardingTarget, Registry, Telemetry,
    WeakEmitter,
};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

const CAPACITY: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

fn fault(code: &str) -> Fault {
    Fault::new(
        code,
        "Actions forwarding could not complete; protection is unchanged.",
        true,
    )
}

fn validate_url(raw: &str, path: &str) -> Result<Url, Fault> {
    let url = Url::parse(raw).map_err(|_| fault("telemetry_invalid_configuration"))?;
    let local = url
        .host_str()
        .and_then(|host| {
            host.strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
                .unwrap_or(host)
                .parse::<std::net::IpAddr>()
                .ok()
        })
        .is_some_and(|ip| ip.is_loopback());
    if !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != path
    {
        return Err(fault("telemetry_invalid_configuration"));
    }
    Ok(url)
}

struct Validated {
    destinations: Destinations,
}

pub(super) fn configure(destinations: Destinations) -> Result<(Arc<Queue>, Worker), Fault> {
    if destinations.analytics.is_none() && destinations.bug_report_url.is_none() {
        return Err(fault("telemetry_invalid_configuration"));
    }
    if let Some(analytics) = &destinations.analytics {
        let origin = validate_url(&analytics.host, "/")?;
        if analytics.project_key.trim().is_empty() || analytics.project_key.len() > 512 {
            return Err(fault("telemetry_invalid_configuration"));
        }
        let resolved = posthog_rs::EndpointManager::new(analytics.host.clone())
            .build_url(posthog_rs::Endpoint::Batch);
        let actual = Url::parse(&resolved).map_err(|_| fault("telemetry_invalid_configuration"))?;
        if origin.origin() != actual.origin() {
            return Err(fault("telemetry_invalid_configuration"));
        }
    }
    if let Some(report) = &destinations.bug_report_url {
        validate_url(report, "/bug-report")?;
    }
    let queue = Arc::new(Queue {
        state: Mutex::new(State {
            jobs: VecDeque::new(),
            stopped: false,
            context: None,
        }),
        changed: Condvar::new(),
        analytics: destinations.analytics.is_some(),
        reports: destinations.bug_report_url.is_some(),
        missing_analytics: AtomicBool::new(false),
        missing_reports: AtomicBool::new(false),
    });
    Ok((
        queue.clone(),
        Worker {
            queue: Some(queue),
            configuration: Some(Validated { destinations }),
            worker: None,
        },
    ))
}

struct Job {
    metadata: Metadata,
    events: WeakEmitter,
}
struct State {
    jobs: VecDeque<Job>,
    stopped: bool,
    context: Option<WeakEmitter>,
}

pub(super) struct Queue {
    state: Mutex<State>,
    changed: Condvar,
    analytics: bool,
    reports: bool,
    missing_analytics: AtomicBool,
    missing_reports: AtomicBool,
}

impl Queue {
    pub(super) fn submit(&self, metadata: Metadata, events: &Emitter) {
        if !self.analytics && !self.missing_analytics.swap(true, Ordering::AcqRel) {
            events.error::<Error::Telemetry::Configuration::Occurred>(fault(
                "analytics_unconfigured",
            ));
        }
        if metadata.recoverable == Some(false)
            && !self.reports
            && !self.missing_reports.swap(true, Ordering::AcqRel)
        {
            events.error::<Error::Telemetry::Configuration::Occurred>(fault(
                "bug_report_unconfigured",
            ));
        }
        if !self.analytics && !(self.reports && metadata.recoverable == Some(false)) {
            return;
        }
        let mut state = self.state.lock().expect("forwarding queue poisoned");
        let code = if state.stopped {
            Some("telemetry_stopped")
        } else if state.jobs.len() == CAPACITY {
            Some("telemetry_queue_full")
        } else {
            None
        };
        if let Some(code) = code {
            drop(state);
            events.error::<Error::Telemetry::Buffer::Occurred>(fault(code));
        } else {
            state.jobs.push_back(Job {
                metadata,
                events: events.downgrade_metadata(),
            });
            self.changed.notify_one();
        }
    }

    pub(super) fn close(&self, context: Option<WeakEmitter>) {
        let mut state = self.state.lock().expect("forwarding queue poisoned");
        state.stopped = true;
        if context.is_some() {
            state.context = context;
        }
        let jobs = std::mem::take(&mut state.jobs);
        self.changed.notify_one();
        drop(state);
        for job in jobs {
            if let Some(events) = job.events.upgrade() {
                events.error::<Error::Telemetry::Buffer::Occurred>(fault("telemetry_abandoned"));
            }
        }
    }

    fn stopped(&self) -> bool {
        self.state
            .lock()
            .expect("forwarding queue poisoned")
            .stopped
    }
}

#[derive(Default)]
pub(crate) struct Worker {
    queue: Option<Arc<Queue>>,
    configuration: Option<Validated>,
    worker: Option<JoinHandle<()>>,
}

impl Worker {
    /// Called only on the already owned owner service thread, never the caller/Actions thread.
    pub(crate) fn start(&mut self, actions: &Registry) {
        let Some(configuration) = self.configuration.take() else {
            return;
        };
        let queue = self.queue.as_ref().expect("configured queue").clone();
        let failed = queue.clone();
        match std::thread::Builder::new()
            .name("screenwake-forwarding".into())
            .spawn(move || run(configuration, queue))
        {
            Ok(worker) => self.worker = Some(worker),
            Err(_) => {
                actions.emit::<Error::Telemetry::Configuration::Occurred>(fault(
                    "telemetry_worker_start",
                ));
                failed.close(None);
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(queue) = &self.queue {
            queue.close(None);
        }
        if let Some(worker) = self.worker.take() {
            // This guard is moved into the owner service before start; no UI/callback ever joins.
            if worker.join().is_err() {
                if let Some(events) = self.queue.as_ref().and_then(|queue| {
                    queue
                        .state
                        .lock()
                        .expect("forwarding queue poisoned")
                        .context
                        .as_ref()
                        .and_then(WeakEmitter::upgrade)
                }) {
                    events.error::<Error::Telemetry::Transport::Occurred>(fault(
                        "telemetry_worker_failed",
                    ));
                }
            }
        }
    }
}

struct Adapter {
    sdk: Option<posthog_rs::Client>,
    http: Client,
    report: Option<String>,
}

impl Adapter {
    fn new(configuration: Validated) -> Result<Self, Fault> {
        let http = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| fault("telemetry_transport_start"))?;
        let sdk = if let Some(analytics) = configuration.destinations.analytics {
            let options = posthog_rs::ClientOptionsBuilder::default()
                .api_key(analytics.project_key)
                .host(analytics.host)
                .blocking_http_client(http.clone())
                .request_timeout_seconds(1_u64)
                .max_capture_attempts(1_u32)
                .max_queue_size(1_usize)
                .max_batch_size(1_usize)
                .shutdown_timeout_ms(0_u64)
                .disable_geoip(true)
                .is_server(false)
                .before_send(|mut event| {
                    let keys: Vec<_> = event.properties().keys().cloned().collect();
                    for key in keys {
                        if ![
                            "action",
                            "recoverable",
                            "$process_person_profile",
                            "$geoip_disable",
                        ]
                        .contains(&key.as_str())
                        {
                            event.remove_prop(&key);
                        }
                    }
                    Some(event)
                })
                .build()
                .map_err(|_| fault("telemetry_transport_start"))?;
            if options.is_disabled() {
                return Err(fault("telemetry_transport_start"));
            }
            Some(posthog_rs::client(options))
        } else {
            None
        };
        Ok(Self {
            sdk,
            http,
            report: configuration.destinations.bug_report_url,
        })
    }

    fn analytics(&self, metadata: &Metadata) -> Result<(), Fault> {
        let mut event = posthog_rs::Event::new_anon("screenwake_action");
        event
            .insert_prop("action", metadata.action)
            .map_err(|_| fault("analytics_encoding_failed"))?;
        if let Some(recoverable) = metadata.recoverable {
            event
                .insert_prop("recoverable", recoverable)
                .map_err(|_| fault("analytics_encoding_failed"))?;
        }
        let summary = self
            .sdk
            .as_ref()
            .expect("configured SDK")
            .capture_immediate(event)
            .map_err(|_| fault("analytics_delivery_failed"))?;
        if summary.submitted() != 1 || !summary.all_persisted() {
            return Err(fault("analytics_delivery_failed"));
        }
        Ok(())
    }

    fn report(&self, metadata: &Metadata) -> Result<(), Fault> {
        let response = self
            .http
            .post(self.report.as_ref().expect("configured report"))
            .json(metadata)
            .send()
            .map_err(|_| fault("bug_report_delivery_failed"))?;
        if !response.status().is_success() {
            return Err(fault("bug_report_delivery_failed"));
        }
        Ok(())
    }
}

fn run(configuration: Validated, queue: Arc<Queue>) {
    let adapter = Adapter::new(configuration);
    loop {
        let mut state = queue.state.lock().expect("forwarding queue poisoned");
        while state.jobs.is_empty() && !state.stopped {
            state = queue
                .changed
                .wait(state)
                .expect("forwarding queue poisoned");
        }
        if state.stopped {
            break;
        }
        let job = state.jobs.pop_front().expect("pending job");
        drop(state);
        for target in [ForwardingTarget::Analytics, ForwardingTarget::BugReport] {
            let selected = match target {
                ForwardingTarget::Analytics => queue.analytics,
                ForwardingTarget::BugReport => {
                    queue.reports && job.metadata.recoverable == Some(false)
                }
            };
            if !selected {
                continue;
            }
            if queue.stopped() {
                if let Some(events) = job.events.upgrade() {
                    events
                        .error::<Error::Telemetry::Buffer::Occurred>(fault("telemetry_abandoned"));
                }
                break;
            }
            let result = match &adapter {
                Ok(adapter) => match target {
                    ForwardingTarget::Analytics => adapter.analytics(&job.metadata),
                    ForwardingTarget::BugReport => adapter.report(&job.metadata),
                },
                Err(error) => Err(error.clone()),
            };
            if let Some(events) = job.events.upgrade() {
                match result {
                    Ok(()) => events.emit::<Telemetry::Delivery::Forwarding::Confirmed>(
                        ForwardingReceipt {
                            source: events.correlation(),
                            target,
                        },
                    ),
                    Err(error) => events.error::<Error::Telemetry::Transport::Occurred>(error),
                }
            }
        }
    }
    drop(adapter);
    let context = queue
        .state
        .lock()
        .expect("forwarding queue poisoned")
        .context
        .clone();
    if let Some(events) = context.and_then(|context| context.upgrade()) {
        events.emit::<Telemetry::Session::Worker::Stopped>(Empty {});
    }
}
// end transport.rs
