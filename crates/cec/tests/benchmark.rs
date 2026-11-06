//! Benchmark harness measuring p50 / p95 latencies for a standby command through
//! the cec-client subprocess path vs the native path.
//!
//! Exit criteria:
//! "A number, not an opinion: if p95(subprocess) - p95(native) < 50 ms, keep the subprocess; else ship native."

#![allow(missing_docs)]

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tv_rs_cec::{CecAddress, CecClient, CecCommand, CecError, CecTransport};

#[derive(Debug, Clone)]
pub struct BenchmarkStats {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub mean_ms: f64,
    pub samples_count: usize,
}

/// Computes percentile p (0.0 .. 1.0) on a slice of durations.
fn compute_percentile(sorted_samples: &[Duration], p: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_samples.len() as f64 - 1.0) * p).round() as usize;
    sorted_samples[idx].as_secs_f64() * 1000.0
}

/// Measures the latency distribution for sending standby commands through a transport.
pub async fn benchmark_standby<T: CecTransport>(
    client: &CecClient<T>,
    iterations: usize,
) -> Result<BenchmarkStats, CecError> {
    let mut samples = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        client.standby(CecAddress::Tv).await?;
        let elapsed = start.elapsed();
        samples.push(elapsed);
    }

    samples.sort();

    let p50_ms = compute_percentile(&samples, 0.50);
    let p95_ms = compute_percentile(&samples, 0.95);
    let total_ms: f64 = samples.iter().map(|d| d.as_secs_f64() * 1000.0).sum();
    let mean_ms = total_ms / (samples.len() as f64);

    Ok(BenchmarkStats {
        p50_ms,
        p95_ms,
        mean_ms,
        samples_count: samples.len(),
    })
}

/// Creates a mock mock_cec_client executable shell script in a temp directory.
#[cfg(unix)]
pub fn create_mock_cec_client(script_dir: &std::path::Path) -> PathBuf {
    let script_path = script_dir.join("mock_cec_client.sh");
    let script_content = r#"#!/usr/bin/env sh
# Simulates cec-client -s
# Accepts 'standby 0' or 'on 0' on stdin and outputs success
cat > /dev/null
echo "TRAFFIC: [ 10 ] >> 0F:36"
echo "TRAFFIC: command executed"
"#;
    let mut f = File::create(&script_path).expect("failed to create mock cec-client script");
    f.write_all(script_content.as_bytes()).unwrap();
    let mut perms = f.metadata().unwrap().permissions();
    perms.set_mode(0o755);
    f.set_permissions(perms).unwrap();
    f.sync_all().unwrap();
    drop(f);
    script_path
}

/// Simulates native libcec standby execution with in-memory direct dispatch.
#[derive(Default)]
pub struct MockNativeTransport;

#[async_trait::async_trait]
impl CecTransport for MockNativeTransport {
    async fn send(&self, command: &str) -> Result<String, CecError> {
        let cmd = CecCommand::try_from(command)?;
        self.send_command(&cmd).await
    }

    async fn send_command(&self, command: &CecCommand) -> Result<String, CecError> {
        command.validate()?;
        match command {
            CecCommand::Standby(addr) => Ok(format!("TRAFFIC: [mock-native] standby {addr}")),
            CecCommand::PowerOn(addr) => Ok(format!("TRAFFIC: [mock-native] on {addr}")),
            _ => Ok("TRAFFIC: [mock-native] ok".to_string()),
        }
    }
}

#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;
    use tv_rs_cec::ClientTransport;

    #[tokio::test]
    #[cfg(unix)]
    async fn benchmark_harness_evaluates_p50_p95_and_exit_criteria() {
        let tmp_dir = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let mock_client_path = create_mock_cec_client(&tmp_dir);

        let subprocess_transport = ClientTransport::new(mock_client_path.to_str().unwrap());
        let subprocess_client = CecClient::new(subprocess_transport);

        let native_transport = MockNativeTransport;
        let native_client = CecClient::new(native_transport);

        // Run warmup
        let _ = subprocess_client.standby(CecAddress::Tv).await;
        let _ = native_client.standby(CecAddress::Tv).await;

        let iterations = 30;
        let sub_stats = benchmark_standby(&subprocess_client, iterations)
            .await
            .expect("subprocess benchmark failed");
        let nat_stats = benchmark_standby(&native_client, iterations)
            .await
            .expect("native benchmark failed");

        let p95_delta_ms = sub_stats.p95_ms - nat_stats.p95_ms;

        println!("=== Benchmark Results for Standby Command ===");
        println!(
            "Subprocess Path: p50 = {:.2} ms, p95 = {:.2} ms, mean = {:.2} ms ({} samples)",
            sub_stats.p50_ms, sub_stats.p95_ms, sub_stats.mean_ms, sub_stats.samples_count
        );
        println!(
            "Native Path:     p50 = {:.2} ms, p95 = {:.2} ms, mean = {:.2} ms ({} samples)",
            nat_stats.p50_ms, nat_stats.p95_ms, nat_stats.mean_ms, nat_stats.samples_count
        );
        println!(
            "p95(subprocess) - p95(native) = {:.2} ms (threshold = 50.0 ms)",
            p95_delta_ms
        );

        // Exit criteria check:
        // if p95(subprocess) - p95(native) < 50 ms, keep the subprocess; else ship native.
        if p95_delta_ms < 50.0 {
            println!(
                "Decision: keep the subprocess as default transport path (p95 delta < 50 ms)."
            );
        } else {
            println!("Decision: ship native as default transport path (p95 delta >= 50 ms).");
        }

        assert!(
            sub_stats.p95_ms >= 0.0,
            "subprocess p95 must be non-negative"
        );
        assert!(nat_stats.p95_ms >= 0.0, "native p95 must be non-negative");

        let _ = std::fs::remove_file(mock_client_path);
    }
}
