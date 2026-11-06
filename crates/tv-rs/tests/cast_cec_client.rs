//! Tests for GoogleCast and HDMI-CEC routing through `TvClient`.

use tv_rs::cec::CecAddress;
use tv_rs::core::{
    AppLaunch, AppRef, Capability, CommandId, DeviceHandle, DeviceOs, Input, InputSelect, Key,
    KeyInput, PowerControl,
};
use tv_rs::{DiscoveredTv, TvClient, TvVendor};

#[test]
fn test_discovered_google_cast_produces_client() {
    let tv = DiscoveredTv::probed(TvVendor::GoogleCast, "192.168.1.100");
    let client = tv.to_client().expect("cast is supported by TvClient");

    assert_eq!(client.vendor(), TvVendor::GoogleCast);
    assert_eq!(client.address(), "192.168.1.100");
    assert_eq!(client.descriptor().os, DeviceOs::GoogleCast);
    assert!(client.core_capabilities().contains(Capability::AppLaunch));
    assert!(client.core_capabilities().contains(Capability::PowerState));
    assert!(client.as_cast().is_some());
}

#[test]
fn test_cec_client_constructor_and_capabilities() {
    let client = TvClient::cec(CecAddress::Tv);

    assert_eq!(client.vendor(), TvVendor::Cec);
    assert_eq!(client.descriptor().os, DeviceOs::Cec);
    assert_eq!(client.as_cec(), Some(CecAddress::Tv));

    let caps = client.core_capabilities();
    assert!(caps.contains(Capability::PowerState));
    assert!(caps.contains(Capability::Wake));
    assert!(caps.contains(Capability::Key));
    assert!(caps.contains(Capability::InputSelect));
}

#[tokio::test]
async fn test_cast_client_app_launch_and_power() {
    let tv = DiscoveredTv::probed(TvVendor::GoogleCast, "127.0.0.1:9");
    let client = tv.to_client().unwrap();

    // AppLaunch launch routes to DIAL client (fails with connection refused on port 9, not UnsupportedDevice/Unsupported)
    let res = client
        .launch(AppRef::new("YouTube"), CommandId::new())
        .await;
    assert!(res.is_err());
    assert!(!matches!(res, Err(tv_rs::TvError::Unsupported(_))));

    // KeyInput is unsupported on Cast
    let key_res = client.key_press(Key::Select, CommandId::new()).await;
    assert!(matches!(key_res, Err(tv_rs::TvError::Unsupported(_))));
}

#[tokio::test]
async fn test_cec_client_power_and_keys() {
    let client = TvClient::cec(CecAddress::Playback1);

    // CEC attempts to drive cec-client subprocess, returning protocol or io error instead of Unsupported
    let wake_res = client.wake().await;
    assert!(wake_res.is_err());
    assert!(!matches!(wake_res, Err(tv_rs::TvError::Unsupported(_))));

    let key_res = client.key_press(Key::Play, CommandId::new()).await;
    assert!(key_res.is_err());
    assert!(!matches!(key_res, Err(tv_rs::TvError::Unsupported(_))));

    let input_res = client.select_input(Input::Hdmi(1), CommandId::new()).await;
    assert!(input_res.is_err());
    assert!(!matches!(input_res, Err(tv_rs::TvError::Unsupported(_))));
}
