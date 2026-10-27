//! Integration tests for port acquisition and locking in `tv-rs-serial`.

#![allow(missing_docs)]

use std::sync::Arc;
use tokio::sync::Barrier;
use tv_rs_core::{TransportError, TvError};
use tv_rs_serial::SerialPortManager;

#[test]
fn test_simultaneous_port_acquisition_returns_port_busy() {
    let manager = SerialPortManager::new();
    let port_path = "/dev/ttyUSB0";

    let guard1 = manager
        .acquire(port_path)
        .expect("first acquisition must succeed");
    assert_eq!(guard1.port_name(), port_path);

    let second_attempt = manager.acquire(port_path);
    assert!(
        matches!(
            second_attempt,
            Err(TvError::Transport(TransportError::PortBusy))
        ),
        "second attempt must return TransportError::PortBusy, got {:?}",
        second_attempt
    );

    drop(guard1);

    let guard2 = manager
        .acquire(port_path)
        .expect("acquisition must succeed after dropping guard1");
    assert_eq!(guard2.port_name(), port_path);
}

#[tokio::test]
async fn test_concurrent_tasks_simultaneous_acquisition_race() {
    let manager = Arc::new(SerialPortManager::new());
    let port_path = "/dev/ttyUSB1";
    let num_tasks = 10;
    let barrier = Arc::new(Barrier::new(num_tasks));

    let mut handles = Vec::new();

    for _ in 0..num_tasks {
        let mgr = Arc::clone(&manager);
        let bar = Arc::clone(&barrier);
        let path = port_path.to_string();

        handles.push(tokio::spawn(async move {
            bar.wait().await;
            mgr.acquire(&path)
        }));
    }

    let mut success_count = 0;
    let mut busy_count = 0;

    for handle in handles {
        let result = handle.await.unwrap();
        match result {
            Ok(_guard) => success_count += 1,
            Err(TvError::Transport(TransportError::PortBusy)) => busy_count += 1,
            Err(other) => panic!("unexpected error variant: {:?}", other),
        }
    }

    assert_eq!(
        success_count, 1,
        "exactly one task must acquire the port lock"
    );
    assert_eq!(
        busy_count,
        num_tasks - 1,
        "all other tasks must receive PortBusy"
    );
}

#[test]
fn test_global_manager_port_locking() {
    let mgr1 = SerialPortManager::global();
    let mgr2 = SerialPortManager::global();

    let _guard = mgr1.acquire("COM3").unwrap();
    let err = mgr2.acquire("COM3").unwrap_err();

    assert!(matches!(err, TvError::Transport(TransportError::PortBusy)));
}
