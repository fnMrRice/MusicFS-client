# MusicFS-client

MusicFS 的桌面客户端仓库，负责桌面交互、播放队列、原生音频输出、桌面歌词和受控的本地 Server 启动。当前处于架构设计阶段，尚无可运行实现。

设计包括：

- 主题、解复用/解码、DSP 和外部控制 `control.api` 插件；PlayerCore 统一管理播放状态、队列与命令。
- Windows Media Foundation（WMF）与内置 FFmpeg 作为音频后端候选，实际能力和打包方式待验证。
- 可配置的切歌淡出、切换、淡入，以及按共同协议处理跨来源恢复。
- 通过 Server 获取音乐库、Provider 推荐和刮削结果，显示来源与任务状态。

桌面 UI 框架、渲染方式及与 Web 的复用方式尚未确定，不预设 Tauri/WebView；原生音频引擎独立于 UI。

文档入口：

- [Client 内部架构](docs/architecture.md)：模块分层、状态管理、连接与平台适配。
- [音频与插件设计](docs/audio-processing.md)：插件契约、音频后端、转场和外部控制。
- [设计导航](docs/design.md)。
- [歌单与来源线框](docs/prototypes/client.salt.puml)、[连接、插件与歌词设置线框](docs/prototypes/settings.salt.puml)：可编辑的 PlantUML Salt 源码，尚非最终界面。
- [跨端架构](../docs/architecture.md)：Server 与客户端共同遵守的协议和行为。
- [工作约定](AGENTS.md)：文档维护与仓库协作规则。

本仓库与 Server、Web 分别管理 Git 历史。跨仓库相对链接按 MusicFS 根目录下 `client`、`server`、`web` 并列检出的工作区布局解析。
