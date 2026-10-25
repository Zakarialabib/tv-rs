# Non-goals

The scope of this workspace, stated as refusals. Every one of these has been asked
for at least once; the answer is "use this crate, then build that yourself".

## This is not a CMS

No playlists, no scheduling, no content management, no accounts. The
[`tv-rs-agent`](crates/agent) crate converges a device to a desired
state that someone else decides.

## This is not a media server

`tv-rs-media` serves bytes it is given (`206`, HLS playlists, path-traversal
rejection). It does not transcode, segment, store or index media.

## This is not a cloud service

LAN only. No relay, no account, no telemetry, no "sign in to control your TV". The
one network dependency is the TV.

## This is not a universal remote

Keys exist so automation can recover a device (wake it, get it out of a modal, bring
an app forward). A D-pad UI, a keyboard screen and a touchpad are the consumer's job.

## This is not HDMI-CEC-first

CEC can power a panel when the network cannot, and it needs an adapter most machines
do not have. It is an optional crate, off by default, never linked into the agent's
default build, and never presented in the README as the fix for an unreachable TV.

## This is not a promise that hardware works

Every protocol crate is written from vendor documentation and public protocol
captures. Device verification is recorded in [`docs/08-hil-results.md`](docs/08-hil-results.md)
with a model, a year and a date — and its absence is stated rather than assumed
away.

## How to propose something adjacent

Open an issue describing the device model change it requires. If it needs a new
capability trait, that is a design discussion. If it needs a scheduler, a player or
a UI, it belongs in your application.
