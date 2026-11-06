# tv-rs-mirror

A video session vocabulary for screen mirroring, not a video pipeline.

As stated in the tv-rs README:

> Does not decode video, render anything, or transcode. It returns URLs and issues commands; what you do with the picture is your call.

- **Numbered sessions**: `0` represents the primary screen; any `n > 0` represents an app window. A missing session defaults to `0` for backwards compatibility with older peers.
- **Control messages**:
  - `mirror.start`: `{ max_size, fps, bitrate, audio?, session?, app?, stay_awake?, screen_off?, watch?, sound_only? }`
  - `mirror.stop`: `{ session? }`
  - `mirror.power`: `{ stay_awake?, screen_off? }`
  - `mirror.keyframe`: `{ session? }`
  - `mirror.resize`: `{ session, width, height }`
  - `mirror.apps`: query and response listing available launchable applications
  - `mirror.sound`: audio streaming toggle
  - `mirror.input`: touch, scroll, key, text, and gamepad forwarding
- **Binary stream packet format**:
  - `u32 length` (big-endian; counts everything after it, max 8 MiB)
  - `u8 kind` (`0` = config, `1` = frame, `2` = keyframe)
  - `u64 time` (capture time in microseconds, big-endian)
  - `data` (config metadata, H.264/H.265 Annex B access units, or PCM audio frames)

This crate provides the types, serializers, deserializers, and validation logic that let a phone or an edge box become a source for a panel that has no network player.
