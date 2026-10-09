# MusicFS-client 音频与插件设计

状态：设计草案；2026-10-09。本文定义 Client 内部插件与音频链，尚无可运行实现。模块边界见[Client 架构](architecture.md)，跨端行为见[共同协议与行为](../../docs/architecture.md)。Server 的处理链、调度与输出缓存见[Server 音频处理设计](../../server/docs/audio-processing.md)。

## 1. 插件宿主与契约

Client Plugin Host 管理 `theme`、`audio.demux`、`audio.decode`、`audio.dsp`、`control.api` 等能力；一个插件可以声明多种能力。内置主题、WMF/FFmpeg 适配器及转场增益也经统一注册表注册。PlayerCore 拥有播放状态、队列、时钟和命令顺序，插件不能直接改写这些状态。设备输出适配方式在平台原型验证后确定。

manifest 声明插件及接口版本、目标平台/架构、能力、输入输出格式、参数 schema、延迟/seek 支持与权限。Host 负责版本协商、配置、启停、取消、错误映射与资源回收；主题和控制插件的故障不能直接进入音频输出回调。Client 与 Server 使用各自的宿主契约，可共享适配代码，不承诺跨运行端、平台或架构的二进制通用。

控制面使用版本化接口，覆盖能力查询、构建/配置、启动、取消、seek/reset、flush/drain、错误与释放。高频音频数据使用有界二进制帧队列或受控共享缓冲，不连续通过 JSON/Base64 传送 PCM。PCM 块明确采样格式、采样率、声道数及布局、帧数、时间戳和时间基准；采样率或声道布局转换由显式节点承担。DSP 报告延迟、尾音和时间映射，PlayerCore 据此维护输出时钟与歌词位置。

进程边界和装载方式仍待验证。第一阶段可内置受信任适配器；第三方原生插件开放前须验证权限与故障隔离，不能仅凭权限清单视为沙箱化。不直接采用跨编译器 C++ ABI；若使用进程内扩展，边界限版本化 C 兼容数据和不透明句柄，语言对象、异常及分配器所有权不得跨界。

## 2. 原生音频链与解码后端

```text
Authorized input -> Demux -> Decode -> PCM
                 -> Client DSP -> Transition gain -> Device output
```

音频引擎独立于桌面 UI 技术，不以 Tauri/WebView 为前提。本机设备输出不需要再编码。Server 已处理的效果通过播放描述进入 Client 的实际生效链；同一效果仅由指定端执行。桌面切歌淡出淡入在 Client 输出链完成，不要求 Server 预先写入音频流。

Windows 的候选后端为 Windows Media Foundation（WMF）：通过 Media Source/Source Reader 读取媒体并使用可用 decoder MFT 获得 PCM；Source Reader 不承担播放器时钟或设备输出。另一条路径为随发行包内置的 FFmpeg 解复用/解码插件。实际格式覆盖取决于系统组件或 FFmpeg 构建，须经过能力探测和打开验证，不宣称任一后端支持全部格式。

可配置 `auto` / `WMF` / `FFmpeg` 及优先级。在打开失败或格式不支持时按已验证能力尝试其他后端，显示失败原因；播放中更换后端由 PlayerCore 取消旧图、清理缓冲后重建，不能混接不同解码状态。后端回退不替代来源切换，也不放宽共同协议中的续播条件。

实时输出回调不执行网络、文件读写或阻塞式插件调用；预分配音频缓冲，由非实时线程解码预取、处理控制请求和重建处理图。Host 限制缓冲和任务数量并处理背压；图更换发生在明确的块/播放边界，旧图不再使用后才能卸载。欠载、超时和崩溃返回明确状态，不得挂住 UI 或访问已释放缓冲。

## 3. 可配置淡出淡入

转场增益插件提供开关、`fadeOutMs`、`fadeInMs`；0 表示对应阶段立即切换。首版建议默认关闭，启用后的初始时长由试听验证确定。首版定义为顺序淡出旧曲、切换、淡入新曲，不承诺双曲同时叠加的交叉混音。

旧曲仍可输出时按配置淡出；新曲有可播放 PCM 后才开始淡入，等待缓冲不消耗淡入时长。新曲未就绪允许静音等待；突发错误无旧音频可用时直接结束旧输出。连续切歌取消过时请求，仅最新播放代次可成为活动曲目；队列、UI、歌词与外部 API 读取同一状态机。

正常换曲与同曲故障恢复分开处理，恢复不冒充换曲而重复触发转场。暂停、退出和设备切换应完成必要的安全收尾，不能为了淡化阻塞停止操作。PlayerCore 负责转场时序和取消，DSP 插件只执行对应增益。

## 4. 播放协议适配

Server 会话的创建、恢复与定位通过协议适配层对接 `/playback/sessions`、`/playback/sessions/{id}/recover`、`/playback/sessions/{id}/seek`（公共 API 前缀见共同协议）；这些是 Server 共同 API，不是下述 Client 控制 API 的路由。会话、资源身份和跨源续播条件以[共同协议与行为](../../docs/architecture.md)为准。

PlayerCore 上报当前尝试、观察到的错误和曲目时间，只应用当前播放代次对应的响应。每次新尝试加载新 URL，清理旧输出缓冲；按共同协议返回的时间映射恢复，不能续接旧字节偏移。条件不满足或定位失败时呈现从头播放/取消选择，不自行猜测位置。Server 的来源重试、转码状态和缓存算法由[Server 音频处理设计](../../server/docs/audio-processing.md)管理。

## 5. 主题插件

主题使用插件注册、安装、启停与版本管理；初期为声明式资源、颜色、字体和布局 token，不执行任意脚本。兼容声明绑定后续选定的 UI 渲染接口。内置默认主题使用相同契约，主题加载失败可回到默认资源；更换主题不得重启音频引擎或修改播放状态。

## 6. 外部控制 API 插件

可选 `control.api` 插件将请求转换为 PlayerCore 类型化命令：`play`、`pause`、`stop`、`prev`、`next`、`seek`、`volume`、`queue` 和 `status`，支持状态查询/订阅。UI、快捷键与 API 共用命令入口和权限检查；请求携带 `commandId`、播放代次及必要的 `expectedRevision`，避免重试和并发导致重复切歌。事件携带序号/修订，断线后可读取状态快照。

外部控制默认关闭；启用后默认仅允许本机访问，网络使用 loopback，本地 IPC 使用同等访问限制。使用独立、可撤销且限定作用范围的 token；HTTP、WebSocket 或 IPC 的选择后续验证。远程控制需显式配置监听范围、鉴权和安全传输；浏览器请求还须验证来源。

API 不提供任意代码/系统命令执行、任意文件读取或 Server/Provider 凭据读取入口；曲目引用仍经过 PlayerCore 与 Server 授权。插件安装、配置及权限管理不随普通播放控制权限开放。停用插件时停止接收请求并释放订阅，不停止正常本机播放。

## 7. 验证重点与技术依据

- 验证 WMF/FFmpeg 的能力探测、错误回退、定位和输出时钟；验证所选构建的格式支持、延迟、资源占用及打包方式。
- 验证不同采样格式和 DSP 延迟处理；确认 Server 与 Client 不重复执行同一效果。
- 验证连续切歌、缓冲等待、恢复、暂停及退出时的转场和取消，只允许最新代次输出。
- 验证插件取消/卸载后无残留音频和失效缓冲访问，主题/API 故障不破坏播放。
- 验证共同协议的资源替换、按时间恢复和无法安全续播时的提示，歌词窗口使用正确的播放时钟。
- 验证 API 与 UI 并发操作、事件重连、本机鉴权、停用/撤销 token 以及远程访问需显式授权。

技术依据：[FFmpeg 媒体处理链](https://ffmpeg.org/ffmpeg.html#Detailed-description)、[Microsoft Source Reader](https://learn.microsoft.com/en-us/windows/win32/medfound/source-reader)、[MFT](https://learn.microsoft.com/en-us/windows/win32/medfound/about-mfts)、[Media Foundation 媒体格式](https://learn.microsoft.com/en-us/windows/win32/medfound/supported-media-formats-in-media-foundation)。这些资料不替代本项目的实际平台验证。
