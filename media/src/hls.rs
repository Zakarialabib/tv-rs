//! HLS manifest generation.
//!
//! TVs perform better on HLS than on raw MP4: the `.m3u8` manifest lets the
//! native decoder pick a bitrate and fetch chunks itself. Manifest construction
//! is pure string work, so it is tested without a server.

use std::fmt::Write as _;

/// One segment of a media playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Segment file name or URL, relative to the manifest.
    pub uri: String,
    /// Segment duration in seconds.
    pub duration_secs: f64,
}

/// A `#EXTM3U` media playlist (`VOD`) built from ordered segments.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaPlaylist {
    /// Segments in playback order.
    pub segments: Vec<Segment>,
    /// `#EXT-X-TARGETDURATION`; rounded up from the longest segment.
    pub target_duration: u64,
}

impl MediaPlaylist {
    /// Builds a playlist from segments, deriving the target duration.
    ///
    /// # Errors
    /// Returns an error string when `segments` is empty — an HLS playlist with
    /// no segments is invalid and players reject it.
    pub fn new(segments: Vec<Segment>) -> Result<Self, String> {
        if segments.is_empty() {
            return Err("an HLS playlist needs at least one segment".to_string());
        }

        let longest = segments
            .iter()
            .map(|segment| segment.duration_secs)
            .fold(0.0_f64, f64::max);
        let target_duration = longest.ceil() as u64;

        Ok(Self {
            segments,
            target_duration,
        })
    }

    /// Renders the `.m3u8` document.
    pub fn render(&self) -> String {
        let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
        let _ = writeln!(out, "#EXT-X-TARGETDURATION:{}", self.target_duration);
        let _ = writeln!(out, "#EXT-X-MEDIA-SEQUENCE:0");
        let _ = writeln!(out, "#EXT-X-PLAYLIST-TYPE:VOD");

        for segment in &self.segments {
            let _ = writeln!(out, "#EXTINF:{:.3},", segment.duration_secs);
            let _ = writeln!(out, "{}", segment.uri);
        }

        out.push_str("#EXT-X-ENDLIST\n");
        out
    }
}

/// Splits a media duration into segment lengths of at most `segment_secs`.
///
/// Used when a single file must be advertised as HLS without pre-segmenting:
/// each entry points at the same file with a byte-range comment left to the
/// player's own range support.
///
/// # Errors
/// Returns an error string when `duration_secs` or `segment_secs` is not
/// positive and finite.
pub fn split_duration(duration_secs: f64, segment_secs: f64) -> Result<Vec<f64>, String> {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err("duration must be a positive number".to_string());
    }
    if !segment_secs.is_finite() || segment_secs <= 0.0 {
        return Err("segment duration must be a positive number".to_string());
    }

    let count = (duration_secs / segment_secs).ceil() as usize;
    let mut durations = Vec::with_capacity(count);
    let mut remaining = duration_secs;
    for _ in 0..count {
        let take = remaining.min(segment_secs);
        durations.push(take);
        remaining -= take;
    }
    Ok(durations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_has_the_required_headers() {
        let playlist = MediaPlaylist::new(vec![
            Segment {
                uri: "seg0.ts".to_string(),
                duration_secs: 6.0,
            },
            Segment {
                uri: "seg1.ts".to_string(),
                duration_secs: 4.0,
            },
        ])
        .expect("valid playlist");

        let rendered = playlist.render();
        assert!(rendered.starts_with("#EXTM3U\n"));
        assert!(rendered.contains("#EXT-X-TARGETDURATION:6"));
        assert!(rendered.contains("#EXTINF:6.000,\nseg0.ts"));
        assert!(rendered.contains("#EXTINF:4.000,\nseg1.ts"));
        assert!(rendered.ends_with("#EXT-X-ENDLIST\n"));
        assert_eq!(playlist.segments.len(), 2);
    }

    #[test]
    fn target_duration_rounds_up() {
        let playlist = MediaPlaylist::new(vec![Segment {
            uri: "a.ts".to_string(),
            duration_secs: 6.2,
        }])
        .expect("valid playlist");
        assert_eq!(playlist.target_duration, 7);
    }

    #[test]
    fn empty_playlist_is_rejected() {
        assert!(MediaPlaylist::new(Vec::new()).is_err());
    }

    #[test]
    fn duration_splits_into_equal_segments() {
        let durations = split_duration(20.0, 6.0).expect("valid split");
        assert_eq!(durations, vec![6.0, 6.0, 6.0, 2.0]);
    }

    #[test]
    fn invalid_durations_are_rejected() {
        assert!(split_duration(0.0, 6.0).is_err());
        assert!(split_duration(10.0, 0.0).is_err());
        assert!(split_duration(f64::NAN, 6.0).is_err());
    }
}
