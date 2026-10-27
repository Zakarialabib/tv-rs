# tv-rs-bluetooth

Bluetooth HID keyboard and BLE remote control emulation for TV devices in the `tv-rs` workspace.

## Overview

Modern smart TVs and streaming boxes (Apple TV, Fire TV Cube, Android TV, Samsung SolarCell remotes) use Bluetooth or BLE instead of traditional IR. By emulating a standard Bluetooth HID Keyboard or Gamepad, `tv-rs-bluetooth` enables sending keystrokes, navigation commands, and text entry directly to TV target devices.

## Features

- **HID Keyboard Reports**: Standard 8-byte USB HID keyboard report encoding.
- **Key Mappings**: Standard mapping from `tv_rs_core::Key` (Up, Down, Left, Right, Select, Back, Home, Volume, Mute, Digits) to USB HID keycodes.
- **Text Input**: ASCII text to HID keycode sequence conversion with Shift modifiers.
- **Device Model Integration**: Implements `tv_rs_core::DeviceHandle`, `KeyInput`, `TextInput`, and `PowerControl`.
- **Hardware-agnostic Transport**: Abstract `BluetoothTransport` trait with built-in `MockBluetoothTransport` for testing without Bluetooth hardware.
