<!-- README.md Copyright (c) 2026:AndrewC. All Rights Reserved. Do Not Distribute. -->

# Backend contracts

The `contract` feature contains the canonical wire payloads, typed action identities and in-process Actions framework. Default features are empty. There is no network server or server framework dependency in this increment.

* `src/contract.rs`: `Power::Session::{TurnOn,TurnOff,Query,Quit,Changed,Snapshot}`, native `Power::Assertions::{Apply,Observed}`, `Power::Activity::{Sample,Observed}`, conditional `Power::Suppression::Recorded`, canonical error hierarchies, Status and native payloads.
* `src/contract/registry.rs`: `RegistryBuilder::{require,on,build}`, `Registry::emit<A>(A::Payload)`, `observe_action<A>(callback)` and `observe(Arc<dyn ObservationSink>)`. `ObservationSink::observed(KvEnvelope)` is the only raw observation shape. Private framework routing derives from the sealed action's Rust hierarchy, never a caller route/verb string.
* Every entry is queued, appended once to the owner-local in-memory canonical log, fanned out to all observers, then delivered to its handler. No callback holds the queue lock. Reentrant emissions and synchronous native completions queue behind the current entry. Fire-and-forget emission has a synchronous in-process drain; it is not a request/response waiter and handlers must not block.
* `Emitter::emit<A>(payload)` continues correlation and `Emitter::error<A>(fault)` records the full failed action before error observers run. Clone a supplied emitter for delayed native completion. One instance allocator is shared across the entire chain, including nested and sibling continuations; durable state replay and cross-process correlation admission remain D3 work.
* `Log::Session::{Query,Snapshot}` is a native-only read-only metadata projection of that same log, including the query. Sequence numbers and framework-formatted display labels avoid recursive log payloads and protected data. The log is not yet durable; the final log window and file/export behavior are not implemented. This is not a fourth MCP tool.
* `IdGeneration::next_id<A>(&payload)` is the only entity mint. SHA-256 uses two-byte big-endian UTF-8 length framing for the hierarchy/payload plus 16 random bytes; checksum chunks retain the full digest, followed by two randomizer chunks. Bus correlation counters are not stored entity identities.
* Raw native input `emit_envelope(KvEnvelope)` admits only the four session controls and the native log query. Native observations require the injected typed callback. Future authenticated IPC must additionally limit admission to the three MCP controls.

Clients depend on `default-features = false, features = ["contract"]`. Wire `_genus` and its delimiter remain implementation details; display labels and `failed_genus` are diagnostics only, never dispatch inputs. The seam does not provide direct state/log getters.

<!-- end README.md -->
