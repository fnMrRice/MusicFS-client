# MusicFS-client 设计入口

当前阶段：已选择 React + Tauri 实施路线，2026-10-10；尚无可运行实现。

Server 与客户端共同遵守的协议和行为由 [MusicFS](../../docs/architecture.md) 管理；OpenAPI、事件协议与协议 SDK 由 [Server 仓库](../../server/)维护。Client 采用 React/TypeScript 界面、Tauri 2 宿主和 Rust 原生核心；与 Web 复用展示组件及适用纯逻辑，原生播放与插件宿主独立实现。具体库、平台能力和协议增量按计划验证。

- [桌面实现方案](implementation.md)：工程、桥接、音频与 C0—C7 阶段。
- [Server 接入与界面复用](implementation/server-integration.md)：原生协商前置项、会话、媒体读取与 Web 迁移。
- [插件实施](implementation/plugins.md)：可安装插件包、动态装载、隔离、卸载与回滚。
- [跨平台交付与验证](implementation/platforms-validation.md)：Windows/macOS/Linux 能力矩阵、制品与证据模板。
- [桌面组件图](diagrams/desktop-components.puml)：尚未渲染验收的 PlantUML 源码。
- [Client 架构](architecture.md)：PlayerCore、UI、插件宿主、连接适配、本地启动器和桌面歌词的分层与职责。
- [音频与插件设计](audio-processing.md)：主题、解复用/解码、DSP、WMF/FFmpeg 候选、可配置淡出淡入及外部控制 API。
- [歌单与来源选择线框](prototypes/client.salt.puml)。
- [连接、插件与歌词设置线框](prototypes/settings.salt.puml)。

线框以 PlantUML Salt 源码交付，布局不是最终视觉设计。协议 SDK 按发布版本关联；若复用 Web 组件或资源，也需记录对应版本。跨仓库变更需声明兼容范围。相对路径按 MusicFS 根目录下的 `client`、`server`、`web` 并列检出布局解析。
