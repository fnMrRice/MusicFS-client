# MusicFS-client

MusicFS 的桌面客户端仓库，负责桌面交互、播放队列、原生音频输出、桌面歌词和受控的本地 Server 启动。当前处于架构设计阶段，尚无可运行实现。

设计包括：

- 主题、解复用/解码、DSP 和外部控制 `control.api` 插件；PlayerCore 统一管理播放状态、队列与命令。
- Windows Media Foundation（WMF）与内置 FFmpeg 作为音频后端候选，实际能力和打包方式待验证。
- 可配置的切歌淡出、切换、淡入，以及按共同协议处理跨来源恢复。
- 通过 Server 获取音乐库、Provider 推荐和刮削结果，显示来源与任务状态。

桌面端已选择 TypeScript + React、Tauri 2 与 Rust 原生核心，目标为 Windows、macOS、Linux。原生音频引擎与用户可安装插件宿主独立于 UI；具体输出库、三平台能力和插件隔离尚待验证。当前 Server 的浏览器音频能力模型需先扩展，才能正式接入原生播放；文档选择不代表已经实现。

文档入口：

- [React + Tauri 实现方案](docs/implementation.md)：工程布局、运行边界、原生音频和 C0—C7 阶段。
- [Server 接入与界面复用](docs/implementation/server-integration.md)、[插件实施](docs/implementation/plugins.md)、[跨平台交付与验证](docs/implementation/platforms-validation.md)。
- [Client 内部架构](docs/architecture.md)：模块分层、状态管理、连接与平台适配。
- [音频与插件设计](docs/audio-processing.md)：插件契约、音频后端、转场和外部控制。
- [设计导航](docs/design.md)。
- [桌面组件图](docs/diagrams/desktop-components.puml)：可编辑的 PlantUML 源码，尚未渲染验收。
- [歌单与来源线框](docs/prototypes/client.salt.puml)、[连接、插件与歌词设置线框](docs/prototypes/settings.salt.puml)：可编辑的 PlantUML Salt 源码，尚非最终界面。
- [跨端架构](../docs/architecture.md)：Server 与客户端共同遵守的协议和行为。
- [工作约定](AGENTS.md)：文档维护与仓库协作规则。

本仓库与 Server、Web 分别管理 Git 历史。跨仓库相对链接按 MusicFS 根目录下 `client`、`server`、`web` 并列检出的工作区布局解析。
