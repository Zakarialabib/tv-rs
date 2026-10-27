//! Vision pipeline coordinator managing stream ingestion, inference execution, and event broadcasting.

use crate::analytics::EventProcessor;
use crate::engine::VisionEngine;
use crate::error::{Result, VisionError};
use crate::stream::RtspStreamReader;
use crate::types::{VisionConfig, VisionEvent};
use tokio::sync::mpsc;
use tracing::{info, warn};

/// High-level async processing pipeline connecting RTSP video streams to AI models and event channels.
pub struct VisionPipeline {
    config: VisionConfig,
    engine: Box<dyn VisionEngine>,
    processor: EventProcessor,
    reader: Option<RtspStreamReader>,
}

impl VisionPipeline {
    /// Creates a new vision pipeline with the specified configuration and inference engine.
    pub fn new(config: VisionConfig, engine: Box<dyn VisionEngine>) -> Self {
        Self {
            config,
            engine,
            processor: EventProcessor::new(),
            reader: None,
        }
    }

    /// Initializes the RTSP stream connection.
    pub async fn connect(&mut self) -> Result<()> {
        let reader = RtspStreamReader::connect(self.config.stream.clone()).await?;
        self.reader = Some(reader);
        Ok(())
    }

    /// Process a single frame step from the RTSP stream and return any resulting vision events.
    pub async fn step(&mut self) -> Result<Vec<VisionEvent>> {
        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| VisionError::RtspStream("Stream not connected".to_string()))?;

        let frame = reader.next_frame().await?;
        let inference_result = self.engine.infer(&frame).await?;
        let events = self
            .processor
            .process(&inference_result, &self.config.zones);

        Ok(events)
    }

    /// Runs the continuous event pipeline, streaming structured `VisionEvent` objects over an mpsc channel.
    pub async fn start(&mut self, buffer_size: usize) -> Result<mpsc::Receiver<VisionEvent>> {
        if self.reader.is_none() {
            self.connect().await?;
        }

        let (tx, rx) = mpsc::channel(buffer_size);
        info!(
            "Starting VisionPipeline using backend '{}'",
            self.engine.backend_name()
        );

        let reader = self.reader.take().unwrap();
        let zones = self.config.zones.clone();
        let mut engine =
            std::mem::replace(&mut self.engine, Box::new(crate::engine::MockEngine::new()));
        let mut processor = EventProcessor::new();

        tokio::spawn(async move {
            let mut stream_reader = reader;
            loop {
                match stream_reader.next_frame().await {
                    Ok(frame) => match engine.infer(&frame).await {
                        Ok(result) => {
                            let events = processor.process(&result, &zones);
                            for event in events {
                                if tx.send(event).await.is_err() {
                                    info!("Vision pipeline event receiver closed");
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Inference error in pipeline loop: {}", e);
                        }
                    },
                    Err(e) => {
                        warn!("Frame fetch error in pipeline loop: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}
