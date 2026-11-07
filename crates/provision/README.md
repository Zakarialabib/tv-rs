# tv-rs-provision

First-boot provisioning and bootstrapping library for smart TVs and companion displays.

## Capabilities

- **BLE Improv Wi-Fi Setup**: State machine for Improv Wi-Fi onboarding over Bluetooth Low Energy.
- **QR Pairing Payloads**: URI framing (`tvrs-companion://`) and crypto tokens for quick commissioning.
- **SAS Verification**: Generates and checks Short Authentication Strings derived from shared secrets.
- **Multicast Lock & Entitlement Gating**: Controls network permissions and discovery locks during bootstrapping.
- **Embedded Compatible**: Works in `no_std` + `alloc` environments.
