# MusicFS-client 架构设计

状态：React + Tauri 实施设计；2026-10-10，尚无可运行实现。本文管理桌面 Client 的内部设计；Server 与客户端共同遵守的 API、数据语义和行为由[共同协议与行为](../../docs/architecture.md)管理。

## 1. 职责与边界

Client 负责桌面交互、播放队列与原生音频输出、主题、桌面歌词、外部控制及受控的本地 Server 启动。音乐库、Provider 推荐、匹配与刮削结果通过 Server 协议获取，Client 不另建一套来源解析、音乐匹配或推荐算法。

桌面采用 TypeScript + React 界面、Tauri 2 宿主与 Rust 原生核心，目标 Windows、macOS、Linux。原生音频引擎独立于 UI，C/C++ 库可通过版本化 C ABI 接入；输出库、解码构建和平台能力仍待原型验证。Web 的展示组件、资源及纯逻辑按版本复用，浏览器音频、同源 fetch 和页面生命周期由桌面适配替换。具体工程和分阶段交付见[实现方案](implementation.md)。

## 2. 内部分层

| 模块 | 职责 | 边界 |
| --- | --- | --- |
| React UI / View Models | 导航、搜索、歌单、来源选择、Provider 推荐、设置及任务进度 | 经 Tauri 桥接消费快照与发送类型化命令，不直接调解码器或启动任意进程 |
| Rust PlayerCore | 队列、播放代次、状态机、时钟、音量、转场及统一命令调度 | 唯一播放状态所有者；UI、快捷键、控制 API 共用入口，不依赖窗口存活 |
| Native Audio Engine | 构建音频图、解码预取、缓冲、DSP、转场增益与设备输出 | 输出回调与 UI/网络/阻塞控制调用隔离，使用 PlayerCore 决定的尝试和配置 |
| Client Plugin Host | 发现、校验、配置和管理主题、解复用/解码、DSP、外部控制插件 | 控制能力与资源范围，不赋予插件直接改写核心状态的权限 |
| Server Connection / Protocol Adapter | 连接配置、认证、版本检查、协议 SDK、事件恢复、播放会话适配 | 隔离 Server 协议与 UI，丢弃已取消播放代次的响应 |
| Managed-local Launcher | 本地实例发现、受控启动、就绪与退出通知、升级协调入口 | 仅在配置允许的模式启动，通过固定目标和参数白名单调用 |
| Lyrics / Window Adapter | 歌词状态、当前播放时间、独立窗口及平台窗口行为 | 使用 PlayerCore 时钟和 Server 选定内容，不独立刮削或猜测跨源时间映射 |
| Settings / Credential Store | 连接与插件配置、主题和转场偏好、受限凭据 | 机密存储与普通设置分离，不将凭据暴露给主题或外部控制调用方 |

插件能力、数据通道、WMF/FFmpeg 候选、淡出淡入和控制 API 见[音频与插件设计](audio-processing.md)。内置插件也遵守相同契约；全面插件化不意味着将权限检查、播放状态和生命周期责任交给任意插件。

Tauri 框架插件与 MusicFS 用户安装插件分开管理。受信任内置音频适配器可进程内运行；第三方原生插件通过独立 PluginWorker 方案验证后开放，进程隔离不自动提供权限沙箱。React/Tauri 通道只传命令、状态与摘要，PCM 留在原生有界队列或受控共享缓冲。详见[插件实施](implementation/plugins.md)和[桌面组件图](diagrams/desktop-components.puml)。

## 3. 交互与状态

UI、快捷键和 `control.api` 将操作提交到 PlayerCore；PlayerCore 完成鉴权、参数与修订检查后更新状态，并向视图和订阅者发布统一快照/事件。异步加载和切歌绑定播放代次，取消后的网络响应、解码输出和歌词结果不得覆盖当前曲目。音量和队列等并发修改使用明确的命令语义，不能由多个界面各自维护可写副本。

歌单、搜索、匹配候选、Provider 推荐和刮削工作流通过协议适配层请求 Server。界面显示来源、账号状态、逐项失败与人工选择；打开推荐项与导入到用户歌单分为独立操作。Client 只维护必要的展示缓存和本地偏好，Server 仍是音乐库与服务端任务状态的事实来源。

## 4. 连接与本地生命周期

连接适配层处理 remote、external-local、managed-local 三种模式，校验 Server 身份和协议兼容范围后建立授权连接。仅明确配置为 managed-local 时允许自动启动；remote 与 external-local 只连接。实例实际启动归属、租约、多个 Client 并存和升级协调遵守[共同协议与行为](../../docs/architecture.md)。

启动器先检查现有实例，再获取用户级启动互斥锁并二次检查；结合受限进程身份信息、配置与端口身份判断是否可启动。健康检查失败不等于进程未运行，端口被其他进程占用时报告冲突；就绪须校验产品身份、协议与实例标识并完成授权，不能只检查 HTTP 200。

仅运行明确配置或随发行包验证的 Server，参数使用白名单，不向 UI/API 暴露任意命令执行。就绪等待采用带退避的有界超时，首版建议总计 15 秒；失败提供日志入口与重试，不无界派生新进程。不能自动提权启动系统服务，也不能直接覆盖运行中的可执行文件。独立启动的 Server 由其部署方式维护；Client 提示版本不兼容，不自动接管。

协议 SDK 按发布版本关联，网络断开后重新读取权威快照并恢复订阅。账号访问凭据、Client 对 Server 的会话和外部控制 token 分开存储与撤销；日志不记录 token、敏感请求头或私有来源地址。

Rust 适配器持有私有 Cookie/CSRF、SSE 和续租任务；WebView 不保存会话秘密，也不直接访问受鉴权音频。当前 Server 的 `htmlAudio` 能力/传输限制须先由 Server 扩展并保持 Web 兼容，不能冒报浏览器能力；精确前置项见[Server 接入](implementation/server-integration.md)。

## 5. 主题、歌词与平台适配

主题使用声明式资源与布局 token，绑定版本化 React 主题接口；主题更换只影响呈现。桌面歌词为独立窗口，置顶、穿透、锁定、缩放等交由平台适配层。Windows、macOS、Linux 分别验证，Linux X11/Wayland 分列证据；能力不支持时保留窗口内歌词等明确退路，详见[平台矩阵](implementation/platforms-validation.md)。

歌词内容及当前版本由 Server 返回，歌词显示与 PlayerCore 的实际输出时钟保持一致；来源/资产/处理链变化后的有效性遵守共同协议。用户歌词偏移是显式设置，不能作为自动确认不同来源时间轴一致的证据。窗口开关或样式变化不得重启音频引擎。

## 6. 原型与验证

按[实现方案](implementation.md)先验证 React/Tauri 三平台运行、PlayerCore 与 UI 解耦、真实插件装载/取消、FFmpeg 及设备输出、转场与歌词时钟，再固定依赖和平台支持范围。并行验证多实例启动、本地连接恢复、窗口能力和外部控制权限；已选择技术路线，但在实测前不承诺格式全覆盖、平台功能一致或第三方二进制兼容。

交互草案见[歌单与来源线框](prototypes/client.salt.puml)和[连接、插件与歌词设置线框](prototypes/settings.salt.puml)。PlantUML Salt 源码可编辑，线框不是最终视觉设计或已实现功能；跨仓库变更需评估共同协议与客户端兼容性。
