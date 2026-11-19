// idle_veto.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Native reply authority; callbacks never wait for owner/core locks or the bus.

use crate::{corners::CornerReadiness, lifecycle::SleepDecision};
use ai2screenwake_backend::contract::{Apply, Fault, WeakEmitter};
use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, Ordering},
};

/// Immutable provenance retained only for an actually ready native acquisition.
pub(crate) struct Admission {
    pub request: Apply,
    pub events: WeakEmitter,
    source: u64,
    corners: Option<CornerReadiness>,
}

#[derive(Default)]
struct State {
    admitted: Option<Apply>,
    epoch: Option<Arc<AtomicBool>>,
    ready: Option<Arc<Admission>>,
    sleeping: bool,
}

/// Only admission/retirement and the short OS reply use this fence.
///
/// The callback tries once, never waits; contention is a capability transition,
/// not a reason to block the OS. Ordinary queries never acquire this fence.
#[derive(Default)]
pub(crate) struct IdleVeto(Mutex<State>);

impl IdleVeto {
    /// Replace admission only for a different owner operation; never revive a retired epoch.
    pub fn authorize(&self, request: Option<Apply>) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.admitted == request {
            return;
        }
        Self::invalidate(&mut state);
        state.admitted = request;
        state.epoch = state
            .admitted
            .as_ref()
            .map(|_| Arc::new(AtomicBool::new(true)));
    }

    fn invalidate(state: &mut State) {
        if let Some(epoch) = &state.epoch {
            epoch.store(false, Ordering::Release);
        }
        state.ready = None;
    }

    pub fn retire(&self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        Self::invalidate(&mut state);
    }

    /// Reject later use of this request without retiring an unrelated replacement.
    pub fn retire_request(&self, request: &Apply) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.admitted.as_ref() == Some(request) {
            Self::invalidate(&mut state);
        }
    }

    /// Capture before native work so concurrent retirement fences its late completion.
    pub fn epoch(&self, request: &Apply) -> Result<Option<Arc<AtomicBool>>, Fault> {
        let state = self.0.lock().map_err(|_| Self::unavailable())?;
        Ok((state.admitted.as_ref() == Some(request))
            .then(|| state.epoch.clone())
            .flatten()
            .filter(|epoch| epoch.load(Ordering::Acquire)))
    }

    fn unavailable() -> Fault {
        Fault::new(
            "idle_veto_unavailable",
            "Native idle-sleep reply authority is unavailable.",
            false,
        )
    }

    pub fn check(&self) -> Result<(), Fault> {
        if self.0.is_poisoned() {
            Err(Self::unavailable())
        } else {
            Ok(())
        }
    }

    /// Publish only after both assertions, source readiness and native timing succeeded.
    pub fn ready(
        &self,
        epoch: Option<Arc<AtomicBool>>,
        request: Apply,
        events: WeakEmitter,
        source: u64,
        corners: Option<CornerReadiness>,
    ) {
        let Some(epoch) = epoch else { return };
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.admitted.as_ref() == Some(&request)
            && state
                .epoch
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &epoch))
            && epoch.load(Ordering::Acquire)
        {
            state.ready = Some(Arc::new(Admission {
                request,
                events,
                source,
                corners,
            }));
        }
    }

    /// Suspend during forced sleep; a matching valid wake may resume the same acquisition.
    pub fn sleeping(&self, sleeping: bool) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.sleeping = sleeping;
    }

    /// The closure performs only the immediate native reply to its captured OS token.
    pub fn reply(
        &self,
        source: u64,
        reply: impl FnOnce(SleepDecision) -> Result<(), Fault>,
    ) -> (Option<Arc<Admission>>, Result<(), Fault>) {
        let state = match self.0.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return (None, reply(SleepDecision::Allow)),
            Err(TryLockError::Poisoned(error)) => {
                let ready = error.into_inner().ready.clone();
                return (
                    ready,
                    reply(SleepDecision::Allow).and(Err(Self::unavailable())),
                );
            }
        };
        let ready = state.ready.as_ref().filter(|ready| {
            ready.source == source
                && !state.sleeping
                && state
                    .epoch
                    .as_ref()
                    .is_some_and(|epoch| epoch.load(Ordering::Acquire))
                && ready.corners.as_ref().is_none_or(CornerReadiness::ready)
        });
        if let Some(ready) = ready {
            let result = reply(SleepDecision::Cancel);
            (Some(ready.clone()), result)
        } else {
            (None, reply(SleepDecision::Allow))
        }
    }
}
// end idle_veto.rs
