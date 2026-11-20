# 桌面播放器验证

2026-11-20 在 Windows x64 上验证，使用 Rust 1.100.0（MSVC）、eframe/egui 0.31.1 和 rodio 0.21.1。仓库内的 Cargo.lock 固定依赖版本。

| 场景 | 验证方式和结果 |
| --- | --- |
| 生产界面和原生应用 | 从仓库根目录执行 `cargo build --release --locked -p musicfs-desktop` 生成 Windows x64 应用。窗口和控件由 egui/eframe 原生绘制。 |
| Windows 界面操作 | 通过 Windows UI Automation 操作实际应用窗口，登录后读到五首扫描曲目，完成搜索、收藏及读回、新建歌单和发布快照，打开推荐来源与 CD 作品库，并从曲表播放和暂停 WAV。[实机曲库截图](../screenshots/windows-library.png) |
| 服务端连接 | 使用从 MusicFS Server 项目构建的隔离实例、随机管理员令牌和动态回环端口。真实登录、曲库分页接口、搜索、收藏、歌单创建及追加、幂等重试、发布快照并打开歌单、注销均通过。 |
| 多页歌单 | 在隔离实例创建 51 个歌单，通过 egui 的辅助功能接口点击“读取更多目标歌单”。验证编辑器读到第 2 页，刷新列表后仍能打开该页的歌单并读取已发布的曲目。 |
| 原生播放 | Rust 播放器通过控制通道协商 `nativeHttp`，按授权读取固定表示。已验证 WAV、FLAC、VBR MP3 的设备播放；测试实例配置 FFmpeg/ffprobe 提供编码与容器信息。独立测试覆盖 WAV 解码、非零定位及清空队列时丢弃迟到的加载结果。 |
| 作品库 | 使用独立 ASMR 作品验证目录导入、真实包树、按来源点播、授权读取 WAV 和按作品归档播放历史。 |
| 歌词 | Rust 解析测试覆盖多标签、同时间译文、时间偏移及纯文本。 |
| 辅助窗口 | Windows 为[迷你播放器](../screenshots/windows-mini.png)和[桌面歌词](../screenshots/windows-lyrics.png)分别创建独立窗口。暂停在 0:02 时，迷你窗口显示 0:02 / 0:06，歌词窗口显示该位置的当前行与下一行。关闭歌词窗口后主程序继续运行。 |
| 返回主窗口 | egui 控件测试点击“主窗口”，验证向主窗口发送还原与聚焦命令，并关闭迷你窗口。Windows 前台鼠标点击仍待实测。 |
| Windows 安装包 | NSIS 生成当前用户安装包。解包检验可执行文件与 Rust release 的 SHA-256 一致，并确认包含 `vcruntime140.dll`。 |

## 复验

```sh
cargo build --release --locked -p musicfs-desktop
cargo test --locked --workspace
```

服务端往返测试默认跳过。准备含可播放 WAV 的专用测试实例，设置 `MUSICFS_TEST_ENDPOINT` 和 `MUSICFS_TEST_TOKEN`，执行 `cargo test -p musicfs-server-adapter --test session_and_stream isolated_server_round_trip -- --ignored`。测试会在该实例创建歌单并修改测试账号的收藏。多页歌单专项使用回环地址，通过 `cargo test -p musicfs-desktop opens_playlist_from_second_page -- --ignored` 运行，会创建 51 个歌单。需要隔离桌面偏好时，可将 `MUSICFS_DESKTOP_CONFIG_DIR` 指向专用目录。

## 平台范围

Windows x64 完成窗口与设备输出验证。安装包已检查内容，干净系统的安装、卸载仍待验证。推荐页使用隔离实例验证，第三方在线账号的个性化推荐待实测。macOS 和 Linux 使用相同的原生界面和 Rust 核心，仍需对应平台的安装、设备及窗口验收。当前播放器使用系统默认设备，设备独占模式、可安装原生插件、本机 DSP 和托管 Server 启动属于后续阶段。
