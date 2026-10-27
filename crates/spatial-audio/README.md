# tv-rs-spatial-audio

Managing Bluetooth A2DP audio sinks, PulseAudio/PipeWire local streams, directional
sound domes, and ceiling speaker arrays based on customer/listener 3D location.

When a digital signage display or television acts as a screen, audio is dynamically routed
to directional sound domes or ceiling speaker zones depending on where a customer is standing.

## Features

- **3D Spatial Routing**: Distance and gain attenuation calculations for directional sound domes and ceiling speakers.
- **Bluetooth A2DP Management**: Connect and route audio to Bluetooth sound domes or speakers automatically.
- **PulseAudio & PipeWire Support**: Local audio stream management, sink switching, and gain adjustment.
- **Mock & System Backends**: Fully scriptable mock backend for testing plus system shell bridges (`pactl`, `pw-cli`, `bluetoothctl`).
