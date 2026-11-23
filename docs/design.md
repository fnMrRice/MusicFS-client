# MusicFS-client 设计入口

2026-11-20 首次验收 Windows 桌面播放器。2026-11-23 的版本加入来源版本与匹配、目录和插件管理、保持登录及本机音效。已运行的功能与验证环境见[桌面验证记录](validation/desktop.md)。用户安装的桌面原生插件仍处于设计阶段。

Server 与客户端共同遵守的协议和行为由 [MusicFS](../../docs/architecture.md) 管理，OpenAPI、事件协议与协议 SDK 由 [Server 仓库](../../server/)维护。桌面界面用 egui 与 eframe 编写，参考 Web 的布局、页面和交互。客户端使用独立 Rust 模块连接 Server、播放音频；原生插件宿主和其他平台能力按计划单独验证。

- [桌面实现方案](implementation.md)说明原生工程、播放状态与 C0—C7 阶段目标。
- [Server 接入与原生界面](implementation/server-integration.md)说明播放协商、授权会话、媒体读取，以及桌面与 Web 的页面对应关系。
- [插件实施](implementation/plugins.md)：可安装插件包、单进程动态库、重启启停与版本回滚。
- [插件能力与宿主边界](plugin-capabilities.md)：扩展角色、CMake/C++23 与 C ABI、宿主服务及同进程限制。
- [跨平台交付与验证](implementation/platforms-validation.md)：Windows/macOS/Linux 能力矩阵、制品与证据模板。
- [桌面组件图](diagrams/desktop-components.puml)：尚未渲染验收的 PlantUML 源码。
- [Client 架构](architecture.md)：PlayerCore、UI、插件宿主、连接适配、本地启动器和桌面歌词的分层与职责。
- [音频与插件设计](audio-processing.md)：主题、解复用/解码、DSP、WMF/FFmpeg 候选、可配置淡出淡入及外部控制 API。
- 早期[歌单与来源选择线框](prototypes/client.salt.puml)。
- 早期[连接、插件与歌词设置线框](prototypes/settings.salt.puml)。

线框使用 PlantUML Salt 描述设计。桌面参考 Web 的交互和视觉规范，在本仓库维护原生控件。跨仓库协议与资源按版本关联。相对路径按 MusicFS 根目录下的 `client`、`server`、`web` 并列检出布局解析。
