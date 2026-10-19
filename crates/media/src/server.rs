//! The axum server: range-aware file serving and HLS manifest endpoints.
//!
//! Two routes, deliberately small:
//!
//! * `GET /media/{path}` — serves a file, honouring `Range` with
//!   `206 Partial Content` and `Accept-Ranges: bytes` (TV players require it);
//! * `GET /hls/{dir}` — builds a `VOD` `.m3u8` from the `*.ts` segments in a
//!   directory, sorted by name.
//!
//! Path traversal is rejected before any file is opened.

use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;

use crate::hls::{MediaPlaylist, Segment};
use crate::range::{parse_range, RangeError};

/// Server configuration.
#[derive(Debug, Clone)]
pub struct MediaServerConfig {
    /// Directory media files are served from. Never escapes this root.
    pub root: PathBuf,
    /// Advertised segment duration in the generated HLS playlist, in seconds.
    pub segment_secs: f64,
}

impl MediaServerConfig {
    /// Builds a config rooted at `root`, with 6-second HLS segments.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            segment_secs: 6.0,
        }
    }
}

/// Builds the axum router for `config`.
pub fn router(config: MediaServerConfig) -> Router {
    Router::new()
        // axum 0.8 moved wildcard captures to `{*name}`; the 0.7 `*name` form
        // now panics when the router is built.
        .route("/media/{*path}", get(serve_media))
        .route("/hls/{*dir}", get(serve_playlist))
        .with_state(config)
}

/// Binds `addr` and serves until the future is dropped.
///
/// # Errors
/// Returns an error when the address cannot be bound or the server fails.
pub async fn serve(
    config: MediaServerConfig,
    addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(config)).await?;
    Ok(())
}

/// Resolves `relative` under `root`, rejecting traversal and absolute paths.
///
/// Returns `None` when any component is `..`, a root, or a prefix.
fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return None;
    }
    if candidate
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let joined = root.join(candidate);
    joined.starts_with(root).then_some(joined)
}

/// Maps a file extension to a media content type.
fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("mp4") => "video/mp4",
        Some("m4v") => "video/x-m4v",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("ts") => "video/mp2t",
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("mp3") => "audio/mpeg",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

/// `GET /media/{path}` — full body or a `206` range slice.
async fn serve_media(
    State(config): State<MediaServerConfig>,
    AxumPath(relative): AxumPath<String>,
    headers: HeaderMap,
) -> Response {
    let Some(path) = safe_join(&config.root, &relative) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let metadata = match tokio::fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let total_len = metadata.len();

    let content_type = content_type_for(&path);
    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));

    let range_header = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let Some(range_header) = range_header else {
        // No Range: a plain 200 with the whole file.
        return match tokio::fs::File::open(&path).await {
            Ok(file) => {
                let mut headers = response_headers;
                headers.insert(header::CONTENT_LENGTH, HeaderValue::from(total_len));
                let body = Body::from_stream(ReaderStream::new(file));
                (StatusCode::OK, headers, body).into_response()
            }
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        };
    };

    let range = match parse_range(&range_header, total_len) {
        Ok(range) => range,
        Err(RangeError::Unsatisfiable) => {
            let mut headers = response_headers;
            let value = format!("bytes */{total_len}");
            if let Ok(value) = HeaderValue::from_str(&value) {
                headers.insert(header::CONTENT_RANGE, value);
            }
            return (StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response();
        }
        Err(RangeError::Malformed(_)) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if let Err(_error) = file.seek(SeekFrom::Start(range.start)).await {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let limited = file.take(range.len());
    let body = Body::from_stream(ReaderStream::new(limited));

    response_headers.insert(header::CONTENT_LENGTH, HeaderValue::from(range.len()));
    if let Ok(value) = HeaderValue::from_str(&range.content_range(total_len)) {
        response_headers.insert(header::CONTENT_RANGE, value);
    }

    (StatusCode::PARTIAL_CONTENT, response_headers, body).into_response()
}

/// `GET /hls/{dir}` — a `VOD` playlist over the directory's `*.ts` segments.
async fn serve_playlist(
    State(config): State<MediaServerConfig>,
    AxumPath(relative): AxumPath<String>,
) -> Response {
    let Some(dir) = safe_join(&config.root, &relative) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    let mut entries = match tokio::fs::read_dir(&dir).await {
        Ok(entries) => entries,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let mut segments = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        segments.push(Segment {
            uri: format!("/media/{relative}/{name}"),
            duration_secs: config.segment_secs,
        });
    }

    if segments.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    segments.sort_by(|left, right| left.uri.cmp(&right.uri));

    let Ok(playlist) = MediaPlaylist::new(segments) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.apple.mpegurl"),
    );
    (StatusCode::OK, headers, playlist.render()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_rejected() {
        let root = Path::new("/srv/media");
        assert!(safe_join(root, "../etc/passwd").is_none());
        assert!(safe_join(root, "a/../../b").is_none());
        assert!(safe_join(root, "/etc/passwd").is_none());
    }

    #[test]
    fn normal_relative_paths_are_accepted() {
        let root = Path::new("/srv/media");
        assert_eq!(
            safe_join(root, "movies/clip.mp4"),
            Some(PathBuf::from("/srv/media/movies/clip.mp4"))
        );
    }

    #[test]
    fn content_types_cover_tv_formats() {
        assert_eq!(content_type_for(Path::new("a.mp4")), "video/mp4");
        assert_eq!(content_type_for(Path::new("a.ts")), "video/mp2t");
        assert_eq!(
            content_type_for(Path::new("a.m3u8")),
            "application/vnd.apple.mpegurl"
        );
        assert_eq!(
            content_type_for(Path::new("a.bin")),
            "application/octet-stream"
        );
    }
}
