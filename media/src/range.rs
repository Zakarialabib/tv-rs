//! HTTP `Range` header parsing for `206 Partial Content` responses.
//!
//! TV players seek by sending `Range: bytes=START-END`; the server must answer
//! `206` with a matching `Content-Range` instead of the whole file. All logic
//! here is pure so it can be unit-tested without a socket.

use std::fmt;

/// A resolved, inclusive byte range inside a resource of known length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// First byte to send (inclusive).
    pub start: u64,
    /// Last byte to send (inclusive).
    pub end: u64,
}

impl ByteRange {
    /// Number of bytes covered by the range.
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }

    /// Always `false`: a resolved range is never empty.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The `Content-Range` header value for a resource of `total_len` bytes.
    pub fn content_range(&self, total_len: u64) -> String {
        format!("bytes {}-{}/{}", self.start, self.end, total_len)
    }
}

/// Why a `Range` header could not be satisfied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    /// The header is not `bytes=...`, uses multiple ranges, or is malformed.
    Malformed(String),
    /// The requested range starts at or beyond the end of the resource.
    Unsatisfiable,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeError::Malformed(value) => write!(f, "malformed Range header: {value}"),
            RangeError::Unsatisfiable => write!(f, "range is not satisfiable"),
        }
    }
}

impl std::error::Error for RangeError {}

/// Parses a single-range `Range: bytes=START-[END]` header against `total_len`.
///
/// Supported forms — the ones TV players actually send:
///
/// * `bytes=100-` — from 100 to the end;
/// * `bytes=100-199` — an explicit inclusive window;
/// * `bytes=-500` — the final 500 bytes (suffix range).
///
/// Multiple ranges (`bytes=0-1,5-6`) are rejected: no TV player needs them and
/// answering with a single range would be wrong.
pub fn parse_range(header: &str, total_len: u64) -> Result<ByteRange, RangeError> {
    if total_len == 0 {
        return Err(RangeError::Unsatisfiable);
    }

    let spec = header
        .trim()
        .strip_prefix("bytes=")
        .ok_or_else(|| RangeError::Malformed(header.to_string()))?;

    if spec.contains(',') {
        return Err(RangeError::Malformed(format!(
            "multiple ranges are not supported: {header}"
        )));
    }

    let (start_raw, end_raw) = spec
        .split_once('-')
        .ok_or_else(|| RangeError::Malformed(header.to_string()))?;

    let parse = |value: &str| {
        value
            .trim()
            .parse::<u64>()
            .map_err(|_| RangeError::Malformed(header.to_string()))
    };

    let range = match (start_raw.trim().is_empty(), end_raw.trim().is_empty()) {
        // bytes=-N : the final N bytes.
        (true, false) => {
            let suffix = parse(end_raw)?;
            if suffix == 0 {
                return Err(RangeError::Unsatisfiable);
            }
            let start = total_len.saturating_sub(suffix);
            ByteRange {
                start,
                end: total_len - 1,
            }
        }
        // bytes=N- : from N to the end.
        (false, true) => {
            let start = parse(start_raw)?;
            if start >= total_len {
                return Err(RangeError::Unsatisfiable);
            }
            ByteRange {
                start,
                end: total_len - 1,
            }
        }
        // bytes=A-B : an explicit window, clamped to the resource.
        (false, false) => {
            let start = parse(start_raw)?;
            let end = parse(end_raw)?;
            if start > end {
                return Err(RangeError::Malformed(format!(
                    "range start {start} exceeds end {end}"
                )));
            }
            if start >= total_len {
                return Err(RangeError::Unsatisfiable);
            }
            ByteRange {
                start,
                end: end.min(total_len - 1),
            }
        }
        // bytes=- : no bounds at all.
        (true, true) => return Err(RangeError::Malformed(header.to_string())),
    };

    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: u64 = 1000;

    #[test]
    fn open_ended_range_runs_to_the_end() {
        assert_eq!(
            parse_range("bytes=500-", LEN),
            Ok(ByteRange {
                start: 500,
                end: 999
            })
        );
    }

    #[test]
    fn explicit_window_is_inclusive() {
        let range = parse_range("bytes=100-199", LEN).expect("valid range");
        assert_eq!((range.start, range.end), (100, 199));
        assert_eq!(range.len(), 100);
        assert_eq!(range.content_range(LEN), "bytes 100-199/1000");
    }

    #[test]
    fn explicit_window_is_clamped_to_the_resource() {
        let range = parse_range("bytes=900-5000", LEN).expect("valid range");
        assert_eq!((range.start, range.end), (900, 999));
    }

    #[test]
    fn suffix_range_returns_the_tail() {
        let range = parse_range("bytes=-100", LEN).expect("valid range");
        assert_eq!((range.start, range.end), (900, 999));
        assert_eq!(range.len(), 100);
    }

    #[test]
    fn suffix_longer_than_the_file_is_the_whole_file() {
        let range = parse_range("bytes=-5000", LEN).expect("valid range");
        assert_eq!((range.start, range.end), (0, 999));
    }

    #[test]
    fn start_beyond_the_end_is_unsatisfiable() {
        assert_eq!(
            parse_range("bytes=1000-", LEN),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            parse_range("bytes=2000-3000", LEN),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(parse_range("bytes=-0", LEN), Err(RangeError::Unsatisfiable));
    }

    #[test]
    fn empty_resource_is_unsatisfiable() {
        assert_eq!(parse_range("bytes=0-", 0), Err(RangeError::Unsatisfiable));
    }

    #[test]
    fn malformed_headers_are_rejected() {
        for header in [
            "items=0-",
            "bytes=",
            "bytes=-",
            "bytes=a-b",
            "bytes=10-1",
            "bytes=0-1,3-4",
        ] {
            assert!(
                matches!(parse_range(header, LEN), Err(RangeError::Malformed(_))),
                "{header} should be malformed"
            );
        }
    }

    #[test]
    fn last_byte_only() {
        let range = parse_range("bytes=999-999", LEN).expect("valid range");
        assert_eq!(range.len(), 1);
        assert_eq!(range.content_range(LEN), "bytes 999-999/1000");
    }
}
