# MusicFS 桌面客户端

简体中文 · [English](docs/README.en.md)

MusicFS 桌面播放器连接已有的 MusicFS Server。窗口、界面、服务端连接和音频播放均使用 Rust。egui/eframe 绘制原生窗口，界面沿用 Web 的侧栏、曲库和底部播放栏。当前开发版本为 [0.1.0](VERSION)。

![Windows 桌面端曲目列表、作品库导航和底部播放器](docs/screenshots/windows-track-list.png)

## 使用

1. 启动已有的 MusicFS Server。在桌面端输入服务端地址和管理员提供的登录令牌。本机默认地址为 `http://127.0.0.1:8080`，远程连接使用 HTTPS。首次登录默认只在本次会话有效；勾选「在此设备上保持登录」后，客户端将登录凭据保存到系统凭据库，重启时自动连接原实例。退出登录会清除已保存的登录。[登录窗口](docs/screenshots/windows-remember-login.png)
2. 已有曲库可以直接播放。要导入音乐，请用管理员账号打开「设置 → 音乐源」，从服务端允许访问的目录中添加音乐目录，查看扫描进度和曲目统计。远程连接时选择的是服务端上的目录。
3. 在「作品库 → 曲目列表」中按来源、歌手、专辑或流派筛选，或打开「文件夹」逐层浏览。双击曲目开始播放。底部播放栏可切歌、调节音量、打开队列、迷你播放器和桌面歌词。支持字节范围定位的音频可以拖动进度条。

## 曲库与设置

- **曲目与来源版本。** 曲目可以包含多个音源。在「来源版本」中可试听、指定默认来源，或把收藏和歌单条目固定到某个版本。自动选择的条目在以后播放时跟随默认来源。文件夹中的文件则按它自身的来源播放。
- **整理曲库。** 重复识别可生成候选，支持人工比对、关联、排除及批量合并。同一专辑内也可合并曲目，指定主曲目，逐项决定保留的标题、别名、艺人、流派、年份、评分与封面。
- **作品、推荐和歌单。** 从侧栏浏览服务端启用的 CD 专辑、ASMR 或电台作品及音轨。在推荐页按来源查看推荐歌单、刷新及浏览该来源支持的历史记录。歌单可添加曲目，在详情页发布快照后播放。
- **音乐源与插件。** 管理员可配置本地目录的导入规则、监听和专辑刮削顺序，以及服务端插件与实例。在线来源按插件能力提供账号状态、扫码登录或匿名检索。Apple Music 授权会在系统浏览器打开同一服务端的 MusicFS Web，需在浏览器登录同一 MusicFS 账号。
- **播放与音效。** 本机播放 MP3、FLAC 和 WAV。可设置音量、自动音量平衡、跳转与切歌淡变时长，并为下一次播放请求服务端转码。在「音效与输出」中可试听、排序和保存增益、十段均衡、参数均衡、动态压缩和混响，按默认输出设备能力选择声道布局。[播放配置窗口](docs/screenshots/windows-playback-settings.png)
- **外观与语言。** 可选内置主题，并跟随系统语言或指定简体中文、繁體中文、English、日本語。歌词窗口显示服务端为当前音源选择的歌词。关闭辅助窗口后主窗口可继续播放。

## 从源码运行

安装 Rust（rustup 将读取 [rust-toolchain.toml](rust-toolchain.toml)）。Windows 构建使用 Visual Studio 的 MSVC 工具链和 Windows SDK。Linux 需要 X11/Wayland、OpenGL、ALSA 和 D-Bus 开发库、pkg-config 及 CJK 字体；保存登录还需要可用的 Secret Service。

```sh
cargo run --locked --release -p musicfs-desktop
```

在本仓库根目录执行 `cargo build --locked --release -p musicfs-desktop` 生成可执行文件，运行 `cargo test --locked --workspace` 执行测试。在 Windows x64 安装 NSIS 后可运行 `./scripts/package-windows.ps1` 生成当前用户安装包。构建产物位于 `target/release/`。安装包自带所需的 Visual C++ 运行库。直接运行独立的 `musicfs-desktop.exe` 时，系统需要已有 x64 Visual C++ 运行库。

## 设计文档

- [桌面验证记录](docs/validation/desktop.md)列出 Windows x64 的窗口、播放和服务端往返验证，以及尚待实测的平台与设备。
- [桌面架构](docs/architecture.md)与[实现方案](docs/implementation.md)说明界面、原生播放器与 Server 的职责。桌面可安装插件宿主、托管 Server 启动和系统媒体键属于后续计划。
- [连接 MusicFS Server](docs/implementation/server-integration.md)说明登录、播放协商和音频流读取。
- [音频处理](docs/audio-processing.md)与[插件宿主](docs/plugin-capabilities.md)说明解码、音效和设备输出的扩展方式。
- [跨平台验证计划](docs/implementation/platforms-validation.md)列出各系统的打包与设备验证项目。

本项目几乎完全由 AI 生成。项目采用 [GNU GPL v3](LICENSE) 许可证。其他客户端见 [MusicFS 主仓库](https://github.com/fnMrRice/MusicFS)。
