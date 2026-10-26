//! Concurrency and thread-safety tests for fleet agent integration.

use std::sync::Arc;
use tokio::task::JoinSet;

use tv_rs_agent::{decide, Decision, DesiredState, HealerLadder};
use tv_rs_core::{
    descriptor::{DeviceDescriptor, DeviceId, DeviceOs},
    mock::MockBackend,
    DeviceHandle, ObservedField, ObservedState, PowerState, TvError,
};

fn device(index: usize) -> DeviceDescriptor {
    DeviceDescriptor::new(
        DeviceId::new(format!("tv-{index:04}")).unwrap(),
        DeviceOs::WebOs,
        format!("10.0.{}.{}", (index / 250) + 1, (index % 250) + 1),
    )
}

#[tokio::test]
async fn concurrent_fleet_reconciliation_under_high_device_count() {
    let device_count = 200;
    let desired = Arc::new(DesiredState::showing("youtube"));
    let ladder = Arc::new(tokio::sync::Mutex::new(HealerLadder::new(3)));

    let devices: Vec<Arc<MockBackend>> = (0..device_count)
        .map(|idx| Arc::new(MockBackend::new(device(idx))))
        .collect();

    let mut set = JoinSet::new();

    for mock in devices {
        let desired = Arc::clone(&desired);
        let _ladder = Arc::clone(&ladder);

        set.spawn(async move {
            let power = mock
                .as_power()
                .expect("power")
                .power_state()
                .await
                .unwrap_or(PowerState::Unknown);

            let observed = ObservedState {
                power: ObservedField::from_read(power),
                ..ObservedState::nothing_observed()
            };

            let decision = decide(&desired, &observed, mock.capabilities());
            match decision {
                Decision::Launch(app) => {
                    let handle = mock.as_app().expect("app handle");
                    handle
                        .launch(app, tv_rs_core::CommandId::new())
                        .await
                        .expect("launch");
                }
                Decision::Wake => {
                    let power = mock.as_power().expect("power handle");
                    power.wake().await.expect("wake");
                }
                _ => {}
            }
            mock.id()
        });
    }

    let mut completed = 0;
    while let Some(res) = set.join_next().await {
        assert!(res.is_ok());
        completed += 1;
    }

    assert_eq!(completed, device_count);
}

#[tokio::test]
async fn rapid_state_changes_and_disconnections_in_fleet_loop() {
    let device_count = 50;
    let desired = Arc::new(tokio::sync::RwLock::new(DesiredState::showing("netflix")));
    let devices: Vec<Arc<MockBackend>> = (0..device_count)
        .map(|idx| {
            let mock = MockBackend::new(device(idx));
            if idx % 5 == 0 {
                // Simulate disconnected/failing power state
                Arc::new(mock.starting_at(PowerState::Unknown))
            } else {
                Arc::new(mock)
            }
        })
        .collect();

    // Broadcast state updates from a simulated CMS/admin task concurrently
    let desired_updater = Arc::clone(&desired);
    let updater_handle = tokio::spawn(async move {
        for app in ["youtube", "prime", "netflix"] {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            let mut writer = desired_updater.write().await;
            *writer = DesiredState::showing(app);
        }
    });

    let mut set = JoinSet::new();
    for mock in devices {
        let desired = Arc::clone(&desired);
        set.spawn(async move {
            for _tick in 0..5 {
                let current_desired = {
                    let reader = desired.read().await;
                    reader.clone()
                };

                let power = mock
                    .as_power()
                    .expect("power")
                    .power_state()
                    .await
                    .unwrap_or(PowerState::Unknown);

                let observed = ObservedState {
                    power: if power == PowerState::Unknown {
                        ObservedField::unknown()
                    } else {
                        ObservedField::from_read(power)
                    },
                    ..ObservedState::nothing_observed()
                };

                let decision = decide(&current_desired, &observed, mock.capabilities());
                if decision == Decision::ProbePower && power == PowerState::Unknown {
                    // Simulate error path
                    let _err = TvError::protocol("disconnected");
                }
                tokio::task::yield_now().await;
            }
        });
    }

    updater_handle.await.expect("updater finished");
    while let Some(res) = set.join_next().await {
        assert!(res.is_ok());
    }
}
