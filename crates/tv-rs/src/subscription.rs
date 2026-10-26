//! `SubscriptionHandle` for DLNA GENA event push and background renewal loop.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tokio::sync::{broadcast, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use tv_rs_core::{AppRef, ObservedField, ObservedState, PowerState};
use tv_rs_dlna::{
    gena_client, parse_notify_body, Subscription, SubscriptionAction, SubscriptionRegistry,
    TransportEvent, TransportState,
};

use crate::error::Result;
use crate::map;

/// Message payload broadcast to event subscribers.
#[derive(Debug, Clone, PartialEq)]
pub struct FacadeEvent {
    /// Parsed transport event from GENA NOTIFY XML.
    pub transport: TransportEvent,
    /// Updated observed state after processing this event.
    pub state: ObservedState,
}

/// Handle for a active DLNA event subscription with a background renewal tick loop.
pub struct SubscriptionHandle {
    udn: String,
    event_sub_url: String,
    callback_url: String,
    registry: Arc<AsyncMutex<SubscriptionRegistry>>,
    state: Arc<Mutex<ObservedState>>,
    sender: broadcast::Sender<FacadeEvent>,
    renewal_task: Option<JoinHandle<()>>,
}

impl SubscriptionHandle {
    /// Establishes a new subscription and spawns the background renewal loop.
    pub async fn subscribe(
        udn: impl Into<String>,
        event_sub_url: impl Into<String>,
        callback_url: impl Into<String>,
    ) -> Result<Self> {
        let udn = udn.into();
        let event_sub_url = event_sub_url.into();
        let callback_url = callback_url.into();

        let mut registry = SubscriptionRegistry::new();
        registry.track(&udn);

        let subscription = gena_client::subscribe(&event_sub_url, &callback_url)
            .await
            .map_err(|err| map::dlna(&err))?;

        registry.record_success(&udn, subscription);

        let registry = Arc::new(AsyncMutex::new(registry));
        let state = Arc::new(Mutex::new(ObservedState::nothing_observed()));
        let (sender, _) = broadcast::channel(64);

        let task_udn = udn.clone();
        let task_event_url = event_sub_url.clone();
        let task_cb_url = callback_url.clone();
        let task_registry = Arc::clone(&registry);

        let renewal_task = tokio::spawn(async move {
            loop {
                let delay = {
                    let reg = task_registry.lock().await;
                    reg.next_tick_delay(SystemTime::now())
                        .unwrap_or(Duration::from_secs(5))
                };

                tokio::time::sleep(delay).await;

                let now = SystemTime::now();
                let due_actions = {
                    let reg = task_registry.lock().await;
                    reg.due(now)
                };

                for (target_udn, action) in due_actions {
                    if target_udn != task_udn {
                        continue;
                    }
                    match action {
                        SubscriptionAction::Renew => {
                            let sub = {
                                let reg = task_registry.lock().await;
                                reg.get(&target_udn).and_then(|t| t.subscription.clone())
                            };
                            if let Some(sub) = sub {
                                match gena_client::renew(&sub).await {
                                    Ok(renewed) => {
                                        let mut reg = task_registry.lock().await;
                                        reg.record_success(&target_udn, renewed);
                                    }
                                    Err(_) => {
                                        let mut reg = task_registry.lock().await;
                                        reg.record_renew_failure(&target_udn);
                                    }
                                }
                            }
                        }
                        SubscriptionAction::Resubscribe => {
                            match gena_client::subscribe(&task_event_url, &task_cb_url).await {
                                Ok(sub) => {
                                    let mut reg = task_registry.lock().await;
                                    reg.record_success(&target_udn, sub);
                                }
                                Err(_) => {
                                    let mut reg = task_registry.lock().await;
                                    reg.record_subscribe_failure(&target_udn);
                                }
                            }
                        }
                        SubscriptionAction::Abandon | SubscriptionAction::Idle => {}
                    }
                }
            }
        });

        Ok(Self {
            udn,
            event_sub_url,
            callback_url,
            registry,
            state,
            sender,
            renewal_task: Some(renewal_task),
        })
    }

    /// Process an incoming GENA NOTIFY XML payload, update state, and broadcast the event.
    pub fn handle_notify(&self, xml: &str) -> Option<FacadeEvent> {
        let transport = parse_notify_body(xml)?;

        let updated_state = {
            let mut state_guard = self.state.lock().unwrap();

            match transport.state {
                TransportState::Playing
                | TransportState::Transitioning
                | TransportState::Paused => {
                    state_guard.power = ObservedField::from_read(PowerState::On);
                }
                TransportState::Stopped | TransportState::NoMedia => {
                    state_guard.power = ObservedField::from_read(PowerState::Standby);
                }
                TransportState::Unknown => {}
            }

            if let Some(ref uri) = transport.current_uri {
                state_guard.foreground = ObservedField::from_read(AppRef::new(uri));
            }

            state_guard.clone()
        };

        let event = FacadeEvent {
            transport,
            state: updated_state,
        };

        let _ = self.sender.send(event.clone());
        Some(event)
    }

    /// Returns a receiver for listening to broadcasted events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<FacadeEvent> {
        self.sender.subscribe()
    }

    /// Returns the current observed state snapshot.
    pub fn state(&self) -> ObservedState {
        self.state.lock().unwrap().clone()
    }

    /// The tracked UDN.
    pub fn udn(&self) -> &str {
        &self.udn
    }

    /// The target event subscription URL.
    pub fn event_sub_url(&self) -> &str {
        &self.event_sub_url
    }

    /// The local callback URL for event notifications.
    pub fn callback_url(&self) -> &str {
        &self.callback_url
    }

    /// Active subscription details if any.
    pub async fn subscription(&self) -> Option<Subscription> {
        let reg = self.registry.lock().await;
        reg.get(&self.udn).and_then(|t| t.subscription.clone())
    }

    /// Gracefully cancels the renewal loop and unsubscribes from the renderer.
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(task) = self.renewal_task.take() {
            task.abort();
        }

        let sub = {
            let mut reg = self.registry.lock().await;
            reg.forget(&self.udn).and_then(|t| t.subscription)
        };

        if let Some(sub) = sub {
            gena_client::unsubscribe(&sub)
                .await
                .map_err(|err| map::dlna(&err))?;
        }

        Ok(())
    }
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        if let Some(task) = self.renewal_task.take() {
            task.abort();
        }
    }
}
