//! `CastDeviceHandle` implementing erased device handle traits and authentic media dispatch.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tv_rs_core::capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle, InputSelect, KeyInput,
    PowerControl, ScreenCastProvider, TextInput, VolumeControl,
};
use tv_rs_core::command::CommandId;
use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, PowerState};
use tv_rs_core::error::TvError;

use crate::cast::client::{CastClient, DEFAULT_MEDIA_RECEIVER_APP_ID, DEFAULT_RECEIVER_ID};
use crate::cast::dial::DialClient;
use crate::cast::payload::MediaStatusDto;

/// Handle driving a Google Cast device over TLS port 8009 with DIAL fallback on port 8008.
pub struct CastDeviceHandle {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    client: Arc<Mutex<CastClient>>,
    dial: Option<Arc<DialClient>>,
    active_transport_id: Arc<RwLock<Option<String>>>,
    active_media_session_id: Arc<RwLock<Option<i32>>>,
}

impl CastDeviceHandle {
    /// Creates a new `CastDeviceHandle`.
    #[must_use]
    pub fn new(
        descriptor: DeviceDescriptor,
        capabilities: CapabilitySet,
        client: Arc<Mutex<CastClient>>,
        dial: Option<Arc<DialClient>>,
    ) -> Self {
        Self {
            descriptor,
            capabilities,
            client,
            dial,
            active_transport_id: Arc::new(RwLock::new(None)),
            active_media_session_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Accesses the underlying client mutex.
    #[must_use]
    pub fn client(&self) -> Arc<Mutex<CastClient>> {
        self.client.clone()
    }

    /// Accesses the DIAL client if present.
    #[must_use]
    pub fn dial(&self) -> Option<Arc<DialClient>> {
        self.dial.clone()
    }

    /// Resolves the active application's transport id over TLS port 8009.
    pub async fn resolve_transport_id(&self) -> Result<String, TvError> {
        if let Some(id) = self.active_transport_id.read().await.clone() {
            return Ok(id);
        }

        let mut lock = self.client.lock().await;
        let status = lock
            .status()
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;

        if let Some(app) = status.applications.first() {
            let id = app.transport_id.clone();
            *self.active_transport_id.write().await = Some(id.clone());
            Ok(id)
        } else {
            Ok(DEFAULT_RECEIVER_ID.to_string())
        }
    }

    /// Queries media status over TLS port 8009 (genuine wire dispatch).
    pub async fn media_status(&self) -> Result<MediaStatusDto, TvError> {
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let status = lock
            .media_status_full(&transport_id)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;

        *self.active_media_session_id.write().await = Some(status.media_session_id);
        Ok(MediaStatusDto::from(&status))
    }

    /// Resumes media playback over TLS port 8009.
    pub async fn play(&self) -> Result<(), TvError> {
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let media_session_id = match *self.active_media_session_id.read().await {
            Some(id) => id,
            None => {
                let status = lock
                    .media_status_full(&transport_id)
                    .await
                    .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
                *self.active_media_session_id.write().await = Some(status.media_session_id);
                status.media_session_id
            }
        };

        lock.play(&transport_id, media_session_id)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))
    }

    /// Pauses media playback over TLS port 8009.
    pub async fn pause(&self) -> Result<(), TvError> {
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let media_session_id = match *self.active_media_session_id.read().await {
            Some(id) => id,
            None => {
                let status = lock
                    .media_status_full(&transport_id)
                    .await
                    .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
                *self.active_media_session_id.write().await = Some(status.media_session_id);
                status.media_session_id
            }
        };

        lock.pause(&transport_id, media_session_id)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))
    }

    /// Stops media playback over TLS port 8009.
    pub async fn stop_media(&self) -> Result<(), TvError> {
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let media_session_id = match *self.active_media_session_id.read().await {
            Some(id) => id,
            None => {
                let status = lock
                    .media_status_full(&transport_id)
                    .await
                    .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
                *self.active_media_session_id.write().await = Some(status.media_session_id);
                status.media_session_id
            }
        };

        lock.transport(&transport_id, media_session_id, "stop")
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))
    }

    /// Seeks media playback to seconds position over TLS port 8009.
    pub async fn seek(&self, position_seconds: f64) -> Result<(), TvError> {
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let media_session_id = match *self.active_media_session_id.read().await {
            Some(id) => id,
            None => {
                let status = lock
                    .media_status_full(&transport_id)
                    .await
                    .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
                *self.active_media_session_id.write().await = Some(status.media_session_id);
                status.media_session_id
            }
        };

        lock.seek(&transport_id, media_session_id, position_seconds)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))
    }
}

impl DeviceHandle for CastDeviceHandle {
    fn id(&self) -> DeviceId {
        self.descriptor.id.clone()
    }

    fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        Some(self)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        Some(self)
    }

    fn as_screen_cast(&self) -> Option<&dyn ScreenCastProvider> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        None
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        None
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }
}

#[async_trait]
impl PowerControl for CastDeviceHandle {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        let mut lock = self.client.lock().await;
        match lock.status().await {
            Ok(_) => Ok(PowerState::On),
            Err(_) => Ok(PowerState::Standby),
        }
    }

    async fn wake(&self) -> Result<(), TvError> {
        // Cast devices wake on connection / launch
        let mut lock = self.client.lock().await;
        lock.launch(DEFAULT_MEDIA_RECEIVER_APP_ID)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Wake))
    }

    async fn sleep(&self) -> Result<(), TvError> {
        // Stop current active application / session
        let transport_id = self.resolve_transport_id().await?;
        let mut lock = self.client.lock().await;
        let _ = lock.stop(&transport_id).await;
        if let Some(ref dial) = self.dial {
            let _ = dial.stop(DEFAULT_MEDIA_RECEIVER_APP_ID).await;
        }
        *self.active_transport_id.write().await = None;
        *self.active_media_session_id.write().await = None;
        Ok(())
    }
}

#[async_trait]
impl AppLaunch for CastDeviceHandle {
    async fn launch(&self, app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        let mut lock = self.client.lock().await;
        if let Err(e) = lock.launch(&app.id).await {
            if let Some(ref dial) = self.dial {
                dial.launch(&app.id)
                    .await
                    .map_err(|de| crate::map::cast::to_tv_error(&de, Capability::AppLaunch))?;
            } else {
                return Err(crate::map::cast::to_tv_error(&e, Capability::AppLaunch));
            }
        }
        *self.active_transport_id.write().await = None;
        Ok(())
    }

    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        let mut lock = self.client.lock().await;
        let status = lock
            .status()
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Foreground))?;

        Ok(status
            .applications
            .first()
            .map(|a| AppRef::new(&a.app_id).with_name(&a.display_name)))
    }

    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        Ok(vec![
            AppRef::new(DEFAULT_MEDIA_RECEIVER_APP_ID).with_name("Default Media Receiver"),
            AppRef::new("YouTube").with_name("YouTube"),
            AppRef::new("Netflix").with_name("Netflix"),
        ])
    }
}

#[async_trait]
impl VolumeControl for CastDeviceHandle {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        let mut lock = self.client.lock().await;
        let status = lock
            .status()
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Volume))?;
        if let Some(vol) = status.volume {
            if let Some(level) = vol.level {
                return Ok(Some((level * 100.0).round() as u8));
            }
        }
        Ok(None)
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        let mut lock = self.client.lock().await;
        lock.set_volume((level as f64) / 100.0)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Volume))
    }

    async fn mute(&self) -> Result<Option<bool>, TvError> {
        let mut lock = self.client.lock().await;
        let status = lock
            .status()
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Volume))?;
        if let Some(vol) = status.volume {
            return Ok(vol.muted);
        }
        Ok(None)
    }

    async fn set_mute(&self, muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        let mut lock = self.client.lock().await;
        lock.set_mute(muted)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::Volume))
    }
}

#[async_trait]
impl ScreenCastProvider for CastDeviceHandle {
    async fn start_cast(&self, target_url: Option<&str>, _cmd: CommandId) -> Result<(), TvError> {
        let mut lock = self.client.lock().await;
        lock.launch(DEFAULT_MEDIA_RECEIVER_APP_ID)
            .await
            .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;

        if let Some(url) = target_url {
            let status = lock
                .status()
                .await
                .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
            let transport_id = status
                .applications
                .first()
                .map(|a| a.transport_id.as_str())
                .unwrap_or(DEFAULT_RECEIVER_ID);
            lock.load_media(transport_id, url, "video/mp4")
                .await
                .map_err(|e| crate::map::cast::to_tv_error(&e, Capability::ScreenCast))?;
        }
        Ok(())
    }

    async fn stop_cast(&self, _cmd: CommandId) -> Result<(), TvError> {
        self.stop_media().await
    }
}
