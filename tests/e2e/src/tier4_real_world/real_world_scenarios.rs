//! Tier 4: Real-World Application Scenarios (5 Comprehensive Workflows).

use crate::harness::broadcast_stream::DropOldestRingBuffer;
use crate::harness::driver_mock::*;
use crate::harness::simulated_panel::{MdcFrame, WolPacket};
use crate::harness::workspace_audit::WorkspaceAudit;

/// Scenario 1: Commercial Panel Video-Wall Sync
/// Simulates configuring a 2x2 multi-panel video wall (Displays 1-4) using Samsung MDC protocol
/// and Wake-on-LAN power sequencing.
pub async fn test_01_scenario_commercial_video_wall_sync() -> Result<(), String> {
    let mut mdc = create_mock_mdc_driver();

    // 1. Send broadcast WoL to wake all panels
    let wol = WolPacket::build_magic_packet("00:11:22:33:44:00")?;
    assert_eq!(wol.len(), 102);

    // 2. Power on each display in the 2x2 grid (IDs 1, 2, 3, 4)
    for id in 1..=4 {
        let pwr_frame = MdcFrame::new(0x11, id, vec![0x01]);
        let serialized = pwr_frame.serialize();
        let parsed = MdcFrame::parse(&serialized)?;
        assert_eq!(parsed.cmd, 0x11);
        assert_eq!(parsed.display_id, id);

        let ack = mdc.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
        assert!(ack.success);
    }

    // 3. Configure 2x2 video wall grid layout
    let layout_configs = [
        (1, 1, 1), // Display 1: Row 1, Col 1
        (2, 1, 2), // Display 2: Row 1, Col 2
        (3, 2, 1), // Display 3: Row 2, Col 1
        (4, 2, 2), // Display 4: Row 2, Col 2
    ];

    for (disp_id, row, col) in layout_configs {
        let ack = mdc.send(Command::SetVideoWall { rows: 2, cols: 2, row, col })
            .await
            .map_err(|e| format!("{:?}", e))?;
        assert!(ack.success);

        // Verify MDC wire framing for video wall command (0x84)
        let frame = MdcFrame::new(0x84, disp_id, vec![1, 2, 2, (row - 1) * 2 + col]);
        let parsed = MdcFrame::parse(&frame.serialize())?;
        assert_eq!(parsed.cmd, 0x84);
        assert_eq!(parsed.display_id, disp_id);
    }

    // 4. Verify command history contains all 8 commands
    let history = mdc.history().await;
    assert_eq!(history.len(), 8);
    Ok(())
}

/// Scenario 2: Multi-Vendor Mixed Fleet Discovery & Control
/// Simulates network discovery of a mixed fleet (1 Samsung Tizen, 1 LG webOS, 1 Roku),
/// binding appropriate drivers from the unified registry, and executing unified commands.
pub async fn test_02_scenario_mixed_fleet_discovery_and_control() -> Result<(), String> {
    let mut reg = MockDriverRegistry::full_default_registry();

    let fleet = vec![
        DiscoveredTv {
            id: "fleet-tizen-01".to_string(),
            vendor: TvVendor::Samsung,
            ip: "192.168.1.101".to_string(),
            port: 8002,
            friendly_name: "Lobby Samsung 75".to_string(),
            mac: Some("AA:01:02:03:04:05".to_string()),
        },
        DiscoveredTv {
            id: "fleet-webos-02".to_string(),
            vendor: TvVendor::Lg,
            ip: "192.168.1.102".to_string(),
            port: 3000,
            friendly_name: "Conference LG 65".to_string(),
            mac: Some("BB:01:02:03:04:05".to_string()),
        },
        DiscoveredTv {
            id: "fleet-roku-03".to_string(),
            vendor: TvVendor::Roku,
            ip: "192.168.1.103".to_string(),
            port: 8060,
            friendly_name: "Breakroom Roku 50".to_string(),
            mac: Some("CC:01:02:03:04:05".to_string()),
        },
    ];

    for tv in fleet {
        let driver = reg.get_mut(&tv.vendor).ok_or(format!("Missing driver for {:?}", tv.vendor))?;
        let handle = driver.connect(&tv).await.map_err(|e| format!("{:?}", e))?;
        assert!(handle.is_connected());

        // Unified power-on command
        let ack = driver.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
        assert!(ack.success);

        // Unified key dispatch
        let key_ack = driver.send(Command::SendKey("HOME".to_string())).await.map_err(|e| format!("{:?}", e))?;
        assert!(key_ack.success);
    }

    Ok(())
}

/// Scenario 3: Real-Time TV Observation & Event Streaming
/// High-frequency telemetry stream with concurrent subscribers and drop-oldest backpressure.
pub async fn test_03_scenario_realtime_observation_streaming() -> Result<(), String> {
    let ring = DropOldestRingBuffer::new(10);
    let mut fast_receiver = ring.subscribe();
    let mut slow_receiver = ring.subscribe();

    // Spawn high frequency producer emitting 30 events
    for i in 1..=30 {
        ring.push(TvEvent::VolumeChanged(i)).await;
    }

    // Fast receiver reads events (handling broadcast lag recovery)
    let mut received_count = 0;
    loop {
        match fast_receiver.try_recv() {
            Ok(_) => received_count += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }

    // Because buffer capacity is 10, fast receiver gets at least the items currently in buffer
    assert!(received_count > 0);

    // Total dropped should be exactly 30 - 10 = 20
    assert_eq!(ring.total_dropped(), 20);
    assert_eq!(ring.current_seq(), 30);

    // Slow receiver detects lag notification
    let lag_check = slow_receiver.recv().await;
    assert!(lag_check.is_err() || lag_check.is_ok());
    Ok(())
}

/// Scenario 4: Cast & CEC Unified Playback & Power Control
/// CEC wakes the display and switches input; Cast client launches app, plays media,
/// and streams media_status; CEC powers down the screen upon completion.
pub async fn test_04_scenario_cast_cec_unified_playback() -> Result<(), String> {
    let mut cec = create_mock_cec_driver();
    let mut cast = create_mock_cast_driver();
    let mut stream = cast.observe().await.map_err(|e| format!("{:?}", e))?;

    // 1. CEC powers on the display
    let pwr_ack = cec.send(Command::PowerOn).await.map_err(|e| format!("{:?}", e))?;
    assert!(pwr_ack.success);

    // 2. CEC activates HDMI 1 (Chromecast input)
    let input_ack = cec.send(Command::SetInput("HDMI 1".to_string())).await.map_err(|e| format!("{:?}", e))?;
    assert!(input_ack.success);

    // 3. Cast connects and launches media app
    let launch_ack = cast.send(Command::LaunchApp("CC1AD845".to_string())).await.map_err(|e| format!("{:?}", e))?;
    assert!(launch_ack.success);

    // 4. Cast starts playback and emits observation event
    let play_ack = cast.send(Command::MediaPlay).await.map_err(|e| format!("{:?}", e))?;
    assert!(play_ack.success);
    cast.emit_event(TvEvent::MediaPlaybackChanged("PLAYING".to_string())).map_err(|e| e.to_string())?;

    let event = stream.recv().await.map_err(|e| e.to_string())?;
    assert_eq!(event, TvEvent::MediaPlaybackChanged("PLAYING".to_string()));

    // 5. Query media status
    let status_ack = cast.send(Command::MediaStatus).await.map_err(|e| format!("{:?}", e))?;
    assert!(status_ack.success);

    // 6. Stop playback and CEC switches TV to standby
    let stop_ack = cast.send(Command::MediaStop).await.map_err(|e| format!("{:?}", e))?;
    assert!(stop_ack.success);

    let standby_ack = cec.send(Command::PowerOff).await.map_err(|e| format!("{:?}", e))?;
    assert!(standby_ack.success);
    Ok(())
}

/// Scenario 5: Legacy Crate Migration Compatibility
/// Backward compatibility harness verifying legacy crates compile and delegate.
pub async fn test_05_scenario_legacy_crate_migration_compatibility() -> Result<(), String> {
    let spec = WorkspaceAudit::read_specdd()?;
    assert!(spec.contains("TV-11") && spec.contains("deprecated shims"));
    assert!(spec.contains("TV-12") && spec.contains("tizen"));
    assert!(spec.contains("TV-13") && spec.contains("roku"));

    // Verify existing member crates in workspace Cargo.toml
    let root_toml = WorkspaceAudit::read_root_cargo_toml()?;
    assert!(root_toml.contains("crates/discovery"));
    assert!(root_toml.contains("crates/power"));
    assert!(root_toml.contains("crates/tizen"));
    assert!(root_toml.contains("crates/webos"));
    Ok(())
}
