# tv-rs-dlna

DLNA MediaRenderer client, AVTransport SOAP actions, and GENA event push subscription handle.

## Features

- **SSDP Multi-device Discovery**: Finds UPnP MediaRenderer devices on the LAN.
- **AVTransport Control**: Play, pause, stop, seek, and query playback status over SOAP.
- **RenderingControl**: Volume and mute control.
- **GENA Event Push**: `SubscriptionHandle` manages background SUBSCRIBE renewals, lapses, and routes incoming NOTIFY events to an asynchronous event stream.
