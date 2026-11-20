# MusicFS Desktop

[简体中文](../README.md) · English

MusicFS Desktop is a planned music player for Windows, macOS, and Linux. This repository contains its architecture and UI prototypes. There is no runnable desktop release yet. The design uses React and Tauri 2 for the interface, with a Rust core for the playback queue, audio output, and desktop lyrics.

## Design preview

![Track list design showing the library, source filters, and playback controls](prototypes/track-list-preview-v5.png)

The image shows the proposed track list. The [playlist wireframe](prototypes/client.salt.puml) illustrates how listeners would merge playlists and choose an audio source.

## Design documents

- [Desktop architecture](architecture.md) and [implementation plan](implementation.md) describe the responsibilities of the interface, native player, and local server.
- [Server integration](implementation/server-integration.md) covers sign-in, playback negotiation, and audio streaming.
- [Audio processing](audio-processing.md) and the [plugin host](plugin-capabilities.md) describe extensions for decoding, effects, and device output.
- The [cross-platform validation plan](implementation/platforms-validation.md) covers packaging and device testing on each operating system.

This project was generated almost entirely by AI and is licensed under the [GNU GPL v3](../LICENSE). See the [MusicFS project](https://github.com/fnMrRice/MusicFS) for other clients.
