# MusicFS-client 设计入口

当前阶段：设计草案，尚无可运行实现。

Server 与客户端共同遵守的协议和行为由 [MusicFS](../../docs/architecture.md) 管理；OpenAPI、事件协议与协议 SDK 由 [Server 仓库](../../server/)维护。Client 的内部模块和插件设计由本仓库管理，桌面 UI 框架、渲染方式及与 Web 的复用方式待验证后确定。

- [Client 架构](architecture.md)：PlayerCore、UI、插件宿主、连接适配、本地启动器和桌面歌词的分层与职责。
- [音频与插件设计](audio-processing.md)：主题、解复用/解码、DSP、WMF/FFmpeg 候选、可配置淡出淡入及外部控制 API。
- [歌单与来源选择线框](prototypes/client.salt.puml)。
- [连接、插件与歌词设置线框](prototypes/settings.salt.puml)。

线框以 PlantUML Salt 源码交付，布局不是最终视觉设计。协议 SDK 按发布版本关联；若复用 Web 组件或资源，也需记录对应版本。跨仓库变更需声明兼容范围。相对路径按 MusicFS 根目录下的 `client`、`server`、`web` 并列检出布局解析。
