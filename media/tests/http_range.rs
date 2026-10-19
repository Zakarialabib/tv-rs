//! End-to-end HTTP tests: a real socket, a real file, real `Range` requests.
//!
//! Each test binds `127.0.0.1:0`, serves a temporary directory, and speaks
//! HTTP/1.1 by hand so the assertions are about bytes on the wire — which is
//! exactly what a TV player sees.

use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use tv_rs_media::{router, MediaServerConfig};

/// Creates a unique temp directory for one test.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tv-rs-media-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Serves `config` on an ephemeral port and returns the bound address.
async fn spawn_server(config: MediaServerConfig) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let app = router(config);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

/// Sends one HTTP/1.1 request and returns `(status_line, headers, body)`.
async fn request(
    addr: std::net::SocketAddr,
    method_and_path: &str,
    extra_headers: &str,
) -> (String, String, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let request = format!(
        "{method_and_path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n{extra_headers}\r\n"
    );
    stream.write_all(request.as_bytes()).await.expect("write");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");

    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = raw[split + 4..].to_vec();

    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default().to_string();
    let headers = lines.collect::<Vec<_>>().join("\r\n");
    (status, headers, body)
}

#[tokio::test]
async fn full_request_returns_200_and_every_byte() {
    let dir = temp_dir("full");
    let payload: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    std::fs::write(dir.join("clip.mp4"), &payload).expect("write media");

    let addr = spawn_server(MediaServerConfig::new(&dir)).await;
    let (status, headers, body) = request(addr, "GET /media/clip.mp4", "").await;

    assert!(status.contains("200"), "unexpected status line: {status}");
    assert!(headers.to_lowercase().contains("accept-ranges: bytes"));
    assert!(headers.to_lowercase().contains("content-type: video/mp4"));
    assert_eq!(body, payload);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn range_request_returns_206_with_content_range() {
    let dir = temp_dir("range");
    let payload: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    std::fs::write(dir.join("clip.mp4"), &payload).expect("write media");

    let addr = spawn_server(MediaServerConfig::new(&dir)).await;
    let (status, headers, body) = request(addr, "GET /media/clip.mp4", "Range: bytes=100-199\r\n").await;

    assert!(status.contains("206"), "unexpected status line: {status}");
    assert!(
        headers.to_lowercase().contains("content-range: bytes 100-199/1000"),
        "missing or wrong Content-Range: {headers}"
    );
    assert_eq!(body.len(), 100);
    assert_eq!(body, payload[100..200].to_vec());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn open_ended_range_returns_the_tail() {
    let dir = temp_dir("tail");
    let payload: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    std::fs::write(dir.join("clip.mp4"), &payload).expect("write media");

    let addr = spawn_server(MediaServerConfig::new(&dir)).await;
    let (status, headers, body) = request(addr, "GET /media/clip.mp4", "Range: bytes=900-\r\n").await;

    assert!(status.contains("206"), "unexpected status line: {status}");
    assert!(headers
        .to_lowercase()
        .contains("content-range: bytes 900-999/1000"));
    assert_eq!(body, payload[900..].to_vec());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn unsatisfiable_range_returns_416() {
    let dir = temp_dir("416");
    std::fs::write(dir.join("clip.mp4"), vec![0u8; 100]).expect("write media");

    let addr = spawn_server(MediaServerConfig::new(&dir)).await;
    let (status, headers, _body) = request(addr, "GET /media/clip.mp4", "Range: bytes=500-\r\n").await;

    assert!(status.contains("416"), "unexpected status line: {status}");
    assert!(headers.to_lowercase().contains("content-range: bytes */100"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn traversal_attempt_is_rejected() {
    let dir = temp_dir("traversal");
    let addr = spawn_server(MediaServerConfig::new(&dir)).await;

    let (status, _headers, _body) = request(addr, "GET /media/../Cargo.toml", "").await;
    assert!(
        status.contains("400") || status.contains("404"),
        "traversal must not be served: {status}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn hls_playlist_lists_ts_segments_in_order() {
    let dir = temp_dir("hls");
    let movie = dir.join("movie");
    std::fs::create_dir_all(&movie).expect("create movie dir");
    for name in ["seg2.ts", "seg0.ts", "seg1.ts"] {
        std::fs::write(movie.join(name), b"ts").expect("write segment");
    }

    let addr = spawn_server(MediaServerConfig::new(&dir)).await;
    let (status, headers, body) = request(addr, "GET /hls/movie", "").await;

    assert!(status.contains("200"), "unexpected status line: {status}");
    assert!(headers
        .to_lowercase()
        .contains("content-type: application/vnd.apple.mpegurl"));

    let text = String::from_utf8(body).expect("utf-8 manifest");
    assert!(text.starts_with("#EXTM3U\n"));
    let first = text.find("seg0.ts").expect("seg0 listed");
    let second = text.find("seg1.ts").expect("seg1 listed");
    let third = text.find("seg2.ts").expect("seg2 listed");
    assert!(first < second && second < third, "segments out of order:\n{text}");
    assert!(text.contains("#EXT-X-ENDLIST"));

    let _ = std::fs::remove_dir_all(&dir);
}
