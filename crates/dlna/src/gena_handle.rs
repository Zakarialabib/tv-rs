//! The subscription handle: a ticking loop that executes the registry's plan,
//! plus the `NOTIFY` ingestion point that turns events into deliveries.
//!
//! [`SubscriptionRegistry`] is a pure brain: it decides what to do next but
//! performs no I/O (see [`crate::gena_registry`]). This module is the body.
//! A background task drives the loop the registry plans — SUBSCRIBE on attach,
//! renew inside the renewal margin, re-subscribe after a lapse or after the
//! renew-failure budget, and retire the device once the subscribe budget runs
//! out — while [`SubscriptionHandle::ingest_notify`] routes an incoming
//! renderer `NOTIFY` straight to the caller's event stream.
//!
//! # Why a handle and not a `TvClient` arm
//!
//! The precedent is MDC (gap G10e): a MediaRenderer is not a television — no
//! power state, no input, no key surface, no pairing — so it does not join the
//! television facade's vendor arms. A renderer you cast *to*, whose only
//! ongoing relationship is a subscription you keep alive, deserves exactly
//! that: a handle you hold alongside the control surface, not a fourth arm
//! pretending the rest of a television exists.
//!
//! # Who receives the NOTIFY
//!
//! A renderer POSTs a `NOTIFY` to the `CALLBACK` URL the subscriber declared at
//! subscribe time. This crate speaks HTTP as a *client* and hosts no server on
//! purpose, so the callback endpoint itself is caller-owned — any HTTP surface
//! the host composes (a route on the media server, a tiny listener in the
//! companion). When such a route receives a NOTIFY, it hands it over:
//! [`SubscriptionHandle::ingest_notify`] takes the request's `SID` header and
//! body, maps SID back to a device through the registry, parses the
//! doubly-encoded `LastChange` payload, and emits the event. That chain is
//! lossless by construction, which is why events surface **as they arrive**
//! rather than on a poll the caller would have to remember to run.
//!
//! # Failure is reported, not thrown
//!
//! A failed subscribe does not fail the call that issued it. The registry's
//! budget ([`crate::gena_registry::MAX_SUBSCRIBE_FAILURES`] consecutive
//! failures) is the verdict:
//! when it runs out, the handle emits [`RendererEvent::Abandoned`] and the
//! loop retires once nothing is left to drive. One unreachable renderer is a
//! normal morning — turning it into an error path would teach the caller to
//! panic at the most common thing a panel does.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::error::DlnaError;
use crate::gena::TransportEvent;
use crate::gena_client::{renew, subscribe as subscribe_raw, unsubscribe};
use crate::gena_parse::parse_notify_body;
use crate::gena_registry::{SubscriptionAction, SubscriptionRegistry};
use crate::models::Renderer;

/// What the caller learns about a renderer as time passes.
///
/// Delivered through the channel [`SubscriptionHandle::take_events`] hands out.
#[derive(Debug, Clone, PartialEq)]
pub enum RendererEvent {
    /// A transport-state event, exactly as the renderer reported it.
    Transport {
        /// The reporting device's UDN (the registry keyed it by SID first).
        udn: String,
        /// The parsed `LastChange` payload.
        event: TransportEvent,
    },
    /// The handle gave up on this renderer: the subscribe budget (the
    /// registry's [`crate::gena_registry::MAX_SUBSCRIBE_FAILURES`] consecutive
    /// failures) ran out. The device
    /// is no longer subscribed or driven. Re-attach by starting a fresh
    /// handle when it is rediscovered — the same deliberate action as the
    /// registry's own abandon-vs-rediscover split.
    Abandoned {
        /// The device's UDN.
        udn: String,
    },
}

/// The renderer data the loop needs to act on the registry's plan.
///
/// Keyed by UDN; the registry keeps everything else (SID, expiry, failure
/// history), so this side carries only what a re-subscribe needs.
struct RendererEndpoint {
    event_sub_url: String,
}

/// Keeps a renderer's GENA subscription alive and its events flowing.
///
/// Construct with [`SubscriptionHandle::start`]; take the event stream with
/// [`SubscriptionHandle::take_events`]; feed renderer NOTIFYs with
/// [`SubscriptionHandle::ingest_notify`]; end the relationship with
/// [`SubscriptionHandle::shutdown`] (which also `UNSUBSCRIBE`s). Dropping the
/// handle stops the loop but cannot unsubscribe — a network call has no
/// business inside `drop` — so `shutdown` is the polite path whenever the
/// caller can await.
#[must_use]
pub struct SubscriptionHandle {
    events_tx: mpsc::UnboundedSender<RendererEvent>,
    events_rx: Mutex<Option<mpsc::UnboundedReceiver<RendererEvent>>>,
    registry: Arc<Mutex<SubscriptionRegistry>>,
    stop: Arc<Notify>,
    task: Option<JoinHandle<()>>,
}

impl SubscriptionHandle {
    /// Start eventing for every renderer that can be evented, and keep it
    /// running until [`SubscriptionHandle::shutdown`] or drop.
    ///
    /// The initial subscription for each renderer is attempted here, before
    /// returning; a failure is recorded in the registry and the loop retries
    /// on its next tick — so `start` is infallible on purpose. A renderer
    /// that offers no `event_sub_url` is skipped: GENA has nothing to
    /// subscribe to, which is the same precondition
    /// [`subscribe_renderer`](crate::gena_client::subscribe_renderer)
    /// enforces (the renderer stays castable through [`crate::soap`]).
    pub async fn start(
        renderers: impl IntoIterator<Item = Renderer>,
        callback_url: impl Into<String>,
    ) -> Self {
        let callback_url = Arc::new(callback_url.into());
        let registry = Arc::new(Mutex::new(SubscriptionRegistry::new()));
        let endpoints: Arc<Mutex<HashMap<String, RendererEndpoint>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(Notify::new());
        let (events_tx, events_rx) = mpsc::unbounded_channel();

        for renderer in renderers {
            let Some(event_sub_url) = renderer.event_sub_url else {
                continue;
            };
            let udn = renderer.udn.clone();
            registry.lock().expect("registry lock").track(&udn);
            endpoints.lock().expect("endpoints lock").insert(
                udn.clone(),
                RendererEndpoint {
                    event_sub_url: event_sub_url.clone(),
                },
            );

            // One attempt now; the loop handles the rest, including the
            // budget that turns persistent failure into `Abandoned`.
            match subscribe_raw(&event_sub_url, &callback_url).await {
                Ok(subscription) => {
                    registry
                        .lock()
                        .expect("registry lock")
                        .record_success(&udn, subscription);
                }
                Err(_) => {
                    registry
                        .lock()
                        .expect("registry lock")
                        .record_subscribe_failure(&udn);
                }
            }
        }

        let task = tokio::spawn(run_loop(
            callback_url,
            Arc::clone(&registry),
            Arc::clone(&endpoints),
            events_tx.clone(),
            Arc::clone(&stop),
        ));

        Self {
            events_tx,
            events_rx: Mutex::new(Some(events_rx)),
            registry,
            stop,
            task: Some(task),
        }
    }

    /// Take the event stream.
    ///
    /// Call once, promptly: events buffer in the channel from the moment the
    /// handle exists, so nothing is lost by taking the receiver late, but
    /// events emitted after the receiver has been dropped (deliberately
    /// closed by the consumer) are dropped with them. A second call returns
    /// `None` — the stream is handed out once.
    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<RendererEvent>> {
        self.events_rx.lock().expect("events lock").take()
    }

    /// Ingest a renderer `NOTIFY`: map its `SID` to a tracked device, parse
    /// the `LastChange` body, and emit the event on the stream.
    ///
    /// Returns `false`, with no effect on the subscription, when
    ///
    /// - the `SID` names no device this handle tracks — either the handle
    ///   never knew it, or the renderer rotated it at renew time and the new
    ///   one is not recorded yet (the next valid NOTIFY carries it), or
    /// - the body carries no AVTransport state at all — renderers legitimately
    ///   event properties we do not track, and [`parse_notify_body`]
    ///   documents that as normal rather than an error.
    pub fn ingest_notify(&self, sid: &str, body: &str) -> bool {
        let udn = {
            let registry = self.registry.lock().expect("registry lock");
            registry.udn_for_sid(sid)
        };
        let Some(udn) = udn else {
            return false;
        };
        let Some(event) = parse_notify_body(body) else {
            return false;
        };
        // An unreachable consumer means the taker dropped the stream — they
        // stopped listening on purpose, so dropping with them is honest.
        let _ = self.events_tx.send(RendererEvent::Transport { udn, event });
        true
    }

    /// Stop the loop, then `UNSUBSCRIBE` from every still-live subscription.
    ///
    /// The loop is awaited first so it cannot race the unsubscribes with a
    /// last renewal or re-subscribe. The first unsubscribe failure is
    /// returned; a renderer that has gone away answers `412` or nothing at
    /// all, which is the normal form of "already forgotten" and arrives here
    /// as an error the caller may usually ignore.
    pub async fn shutdown(mut self) -> Result<(), DlnaError> {
        self.stop.notify_one();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        let actives = {
            let registry = self.registry.lock().expect("registry lock");
            registry.active_subscriptions()
        };
        let mut first_error = None;
        for (_, subscription) in actives {
            if let Err(error) = unsubscribe(&subscription).await {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // No network in `drop`: signal the loop and abort it if still alive.
        // A caller that can await the politer unsubscribe path uses
        // [`SubscriptionHandle::shutdown`] instead.
        self.stop.notify_one();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// The loop: wake when the registry says something is due, do it, report.
async fn run_loop(
    callback_url: Arc<String>,
    registry: Arc<Mutex<SubscriptionRegistry>>,
    endpoints: Arc<Mutex<HashMap<String, RendererEndpoint>>>,
    events_tx: mpsc::UnboundedSender<RendererEvent>,
    stop: Arc<Notify>,
) {
    // Sleeps are bounded above so a newly tracked device is picked up within
    // a tick even though its own deadline may be half an hour away; the
    // registry clamps its own answer the same way.
    const IDLE_TICK: Duration = Duration::from_secs(60);

    loop {
        let delay = {
            let registry = registry.lock().expect("registry lock");
            registry
                .next_tick_delay(SystemTime::now())
                .unwrap_or(IDLE_TICK)
        };

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = stop.notified() => break,
        }

        let due = {
            let registry = registry.lock().expect("registry lock");
            registry.due(SystemTime::now())
        };

        for (udn, action) in due {
            // Deliberately scoped: network calls happen *outside* the lock, so
            // `ingest_notify` (synchronous, caller threads) is never blocked
            // behind a slow renewal.
            match action {
                SubscriptionAction::Idle => {}
                SubscriptionAction::Renew => {
                    let subscription = {
                        let registry = registry.lock().expect("registry lock");
                        registry
                            .get(&udn)
                            .and_then(|tracked| tracked.subscription.clone())
                    };
                    if let Some(subscription) = subscription {
                        match renew(&subscription).await {
                            // A renew may rotate the SID; the record carries the
                            // fresh one so NOTIFY routing stays aligned.
                            Ok(renewed) => {
                                registry
                                    .lock()
                                    .expect("registry lock")
                                    .record_success(&udn, renewed);
                            }
                            Err(_) => {
                                registry
                                    .lock()
                                    .expect("registry lock")
                                    .record_renew_failure(&udn);
                            }
                        }
                    }
                }
                SubscriptionAction::Resubscribe => {
                    let event_sub_url = {
                        let endpoints = endpoints.lock().expect("endpoints lock");
                        endpoints
                            .get(&udn)
                            .map(|endpoint| endpoint.event_sub_url.clone())
                    };
                    if let Some(event_sub_url) = event_sub_url {
                        match subscribe_raw(&event_sub_url, &callback_url).await {
                            Ok(subscription) => {
                                registry
                                    .lock()
                                    .expect("registry lock")
                                    .record_success(&udn, subscription);
                            }
                            Err(_) => {
                                registry
                                    .lock()
                                    .expect("registry lock")
                                    .record_subscribe_failure(&udn);
                            }
                        }
                    }
                }
                SubscriptionAction::Abandon => {
                    {
                        let mut registry = registry.lock().expect("registry lock");
                        registry.forget(&udn);
                    }
                    endpoints.lock().expect("endpoints lock").remove(&udn);
                    // The budget spoke; the caller hears it as an event, not
                    // an error — one TV being off is not a panic condition.
                    let _ = events_tx.send(RendererEvent::Abandoned { udn });
                }
            }
        }

        // Nothing left to drive — the loop retires instead of idling for ever.
        if registry.lock().expect("registry lock").is_empty() {
            break;
        }
    }
}
