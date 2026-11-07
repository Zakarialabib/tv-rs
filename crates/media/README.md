# tv-rs-media

HTTP media streaming server supporting HTTP 206 Partial Content range requests and HLS playlist serving.

## Overview

Smart TVs (Tizen, webOS, Roku) require compliant HTTP range headers and seekable byte streams for reliable playback. `tv-rs-media` hosts local media files and generates dynamic HLS playlists using `axum`, keeping heavier web server dependencies isolated from the core protocol crates.
