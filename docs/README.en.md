# MusicFS Desktop

[简体中文](../README.md) · English

MusicFS Desktop connects to a running MusicFS Server. The interface, server connection and audio player are written in Rust. Eframe and egui draw the native windows, with navigation and playback controls based on the Web design.

![Windows desktop library and playback controls](screenshots/windows-library.png)

## Getting started

Open MusicFS and sign in with the server URL and administrator token. For a local server the default URL is `http://127.0.0.1:8080`. Use HTTPS for remote servers. The library, search, favorites and playlists use your server's catalog. Double-click a track to play, or open the queue, mini player and lyrics from the main window. Sessions last for the lifetime of the app.

From the repository root, run `cargo run --locked --release -p musicfs-desktop` with the [pinned Rust toolchain](../rust-toolchain.toml). Windows builds require MSVC and the Windows SDK. Linux builds require the system windowing, OpenGL and ALSA development libraries. Install a CJK system font to display Chinese text. On Windows x64, use `./scripts/package-windows.ps1` with NSIS to create an installer. The original [playlist wireframe](prototypes/client.salt.puml) and the documents below describe the longer-term design.

The Windows installer bundles the required Visual C++ runtime. Running the standalone executable requires the x64 Visual C++ runtime on the system.

## Design documents

- [Desktop architecture](architecture.md) and [implementation plan](implementation.md) describe the responsibilities of the interface, native player, and local server.
- [Server integration](implementation/server-integration.md) covers sign-in, playback negotiation, and audio streaming.
- [Audio processing](audio-processing.md) and the [plugin host](plugin-capabilities.md) describe extensions for decoding, effects, and device output.
- The [cross-platform validation plan](implementation/platforms-validation.md) covers packaging and device testing on each operating system.

This project was generated almost entirely by AI and is licensed under the [GNU GPL v3](../LICENSE). See the [MusicFS project](https://github.com/fnMrRice/MusicFS) for other clients.
