# tv-rs-vision (The Eyes)

Computer vision & edge AI event engine for the `tv-rs` ecosystem.

It handles RTSP camera streams, runs edge-optimized AI models (YOLO for object detection,
MediaPipe for pose/gesture tracking), and outputs structured JSON events (e.g.,
`{"event": "person_detected", "zone": "entrance", "count": 3}`).

## Features

- **RTSP Camera Streams:** Async stream ingestion, framing, and simulation.
- **Edge AI Models:** YOLO object detection and MediaPipe pose/gesture estimation wrappers.
- **Zone Analytics:** Named spatial regions, person counts, object tracking, and gesture recognition.
- **Structured JSON Events:** Standardized event formats ready for IPC, MQTT, or HTTP broadcasting.
- **Pure-Rust & Mock Support:** Runs seamlessly in test environments and CI without requiring external C libraries.

## Example

```rust
use tv_rs_vision::{
    VisionConfig, VisionPipeline, MockEngine, Zone, VisionEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = VisionConfig::default();
    config.zones.push(Zone::new("entrance", 0.0, 0.0, 0.5, 1.0));

    let engine = Box::new(MockEngine::new());
    let mut pipeline = VisionPipeline::new(config, engine);

    pipeline.connect().await?;
    let events = pipeline.step().await?;

    for event in events {
        println!("{}", serde_json::to_string(&event)?);
    }

    Ok(())
}
```
