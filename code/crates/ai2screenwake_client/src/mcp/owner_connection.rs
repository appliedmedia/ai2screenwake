// owner_connection.rs Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute.
//! Request correlation only; protection and idle policy remain in the owner.

use super::tools::Control;
use crate::{
    contract::{
        ControlResult, CorrelationId, Fault, KvEnvelope, ObservationSink, OwnerControl,
        OwnerLocation, Power,
    },
    ipc::{MAX_OUTSTANDING, OwnerConnection, for_current_user},
};
use ai2screenwake_native::launch::{BundleLauncher, OwnerLauncher};
use rmcp::{ErrorData, model::ErrorCode};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{Mutex as AsyncMutex, oneshot};

enum Configuration {
    Bundled,
    Explicit(OwnerLocation, Arc<dyn OwnerLauncher>),
}

#[derive(Default)]
struct Pending(Mutex<HashMap<CorrelationId, oneshot::Sender<ControlResult>>>);

impl ObservationSink for Pending {
    fn observed(&self, envelope: KvEnvelope) {
        match envelope.payload::<Power::Session::Settled>() {
            Ok(Some(settled)) => {
                if let Some(sender) = self
                    .0
                    .lock()
                    .expect("MCP correlation poisoned")
                    .remove(&settled.correlation)
                {
                    // A cancelled MCP caller relinquishes notification, not owner intent.
                    let _ = sender.send(settled.result);
                }
            }
            Ok(None) => {}
            Err(fault) => {
                for (_, sender) in self.0.lock().expect("MCP correlation poisoned").drain() {
                    let _ = sender.send(ControlResult {
                        status: None,
                        error: Some(fault.clone()),
                    });
                }
            }
        }
    }
}

struct Connected {
    connection: OwnerConnection,
    pending: Arc<Pending>,
}

struct Interest<'a> {
    correlation: CorrelationId,
    pending: Arc<Pending>,
    connection: &'a OwnerConnection,
}

impl Drop for Interest<'_> {
    fn drop(&mut self) {
        let cancelled = self
            .pending
            .0
            .lock()
            .expect("MCP correlation poisoned")
            .remove(&self.correlation)
            .is_some();
        if cancelled {
            self.connection.abandon(self.correlation.clone());
        }
    }
}

/// Lazy same-session connector; it never owns assertions or calculates status.
pub struct OwnerAdapter {
    configuration: Configuration,
    connected: AsyncMutex<Option<Arc<Connected>>>,
}

impl OwnerAdapter {
    /// Resolve the exact containing app and native login session on first control.
    pub fn bundled() -> Self {
        Self {
            configuration: Configuration::Bundled,
            connected: AsyncMutex::new(None),
        }
    }

    /// Inject an exact authorized owner location and native launch collaborator.
    ///
    /// Integration scenarios use a private actual socket and a test-only launcher.
    /// The shipped binary never reads an override path or substitutes a test owner.
    pub fn for_owner(location: OwnerLocation, launcher: Arc<dyn OwnerLauncher>) -> Self {
        Self {
            configuration: Configuration::Explicit(location, launcher),
            connected: AsyncMutex::new(None),
        }
    }

    async fn connection(&self) -> Result<Arc<Connected>, Fault> {
        let mut connected = self.connected.lock().await;
        if let Some(connection) = &*connected {
            return Ok(connection.clone());
        }
        let (location, launcher): (_, Arc<dyn OwnerLauncher>) = match &self.configuration {
            Configuration::Bundled => {
                let executable = std::env::current_exe().map_err(|_| unavailable())?;
                let bundle = containing_bundle(executable)?;
                let directory = ai2screenwake_native::identity::runtime_directory()?;
                (
                    for_current_user(directory),
                    Arc::new(BundleLauncher::new(bundle)),
                )
            }
            Configuration::Explicit(location, launcher) => (location.clone(), launcher.clone()),
        };
        let connection = OwnerConnection::connect(location, launcher).map_err(|_| {
            Fault::new(
                "owner_connection_failed",
                "The owner connector could not start.",
                true,
            )
        })?;
        let pending = Arc::new(Pending::default());
        connection.observe(pending.clone());
        let connection = Arc::new(Connected {
            connection,
            pending,
        });
        *connected = Some(connection.clone());
        Ok(connection)
    }

    pub(super) async fn call(&self, control: Control) -> Result<ControlResult, ErrorData> {
        let connection = match self.connection().await {
            Ok(connection) => connection,
            Err(fault) => {
                return Ok(ControlResult {
                    status: None,
                    error: Some(fault),
                });
            }
        };
        match control {
            Control::Query => connection.call::<Power::Session::Query>().await,
            Control::TurnOn => connection.call::<Power::Session::TurnOn>().await,
            Control::TurnOff => connection.call::<Power::Session::TurnOff>().await,
        }
    }
}

impl Connected {
    async fn call<C: OwnerControl>(&self) -> Result<ControlResult, ErrorData> {
        let prepared = match self.connection.prepare::<C>() {
            Ok(prepared) => prepared,
            Err(fault) => {
                return Ok(ControlResult {
                    status: None,
                    error: Some(fault),
                });
            }
        };
        let correlation = prepared.correlation();
        let (sender, receiver) = oneshot::channel();
        {
            let mut pending = self.pending.0.lock().expect("MCP correlation poisoned");
            if pending.len() >= MAX_OUTSTANDING {
                return Err(ErrorData::new(
                    ErrorCode(-32000),
                    "Too many outstanding MCP controls.",
                    None,
                ));
            }
            pending.insert(correlation.clone(), sender);
        }
        let _interest = Interest {
            correlation,
            pending: self.pending.clone(),
            connection: &self.connection,
        };
        self.connection.emit(prepared);
        receiver.await.map_err(|_| {
            ErrorData::internal_error("Owner observation closed before settlement.", None)
        })
    }
}

fn unavailable() -> Fault {
    Fault::new(
        "owner_bundle_unavailable",
        "Run the MCP executable from its AI2ScreenWake app bundle.",
        true,
    )
}

fn containing_bundle(executable: PathBuf) -> Result<PathBuf, Fault> {
    let executable = executable.canonicalize().map_err(|_| unavailable())?;
    let macos = executable.parent().ok_or_else(unavailable)?;
    let contents = macos.parent().ok_or_else(unavailable)?;
    let bundle = contents.parent().ok_or_else(unavailable)?;
    if executable
        .file_name()
        .is_none_or(|name| name != "ai2screenwake-mcp")
        || macos.file_name().is_none_or(|name| name != "MacOS")
        || contents.file_name().is_none_or(|name| name != "Contents")
        || bundle
            .extension()
            .is_none_or(|extension| extension != "app")
    {
        return Err(unavailable());
    }
    Ok(bundle.to_path_buf())
}
// end owner_connection.rs
