# MusicFS-client 音频与插件设计

本文定义 Client 内部插件与音频链的长期设计。首个原生播放实现采用 rodio，已接入的能力见[桌面验证记录](validation/desktop.md)。模块边界见[Client 架构](architecture.md)，工程与阶段见[实现方案](implementation.md)，跨端行为见[共同协议与行为](../../docs/architecture.md)。Server 的处理链、调度与输出缓存见[Server 音频处理设计](../../server/docs/playback/audio-processing.md)。

## 1. 插件宿主与契约

Client Plugin Host 管理 `theme`、`audio.demux`、`audio.decode`、`audio.dsp`、`control.api` 等能力；一个插件可以声明多种能力。内置主题、WMF/FFmpeg 适配器及转场增益也经统一注册表注册。继续以 `audio.output` 扩展设备后端，以 `audio.visualizer` 扩展音频分析，以 `control.integration` 扩展系统/外部控制接入；新角色的接口与支持阶段见[插件能力与宿主边界](plugin-capabilities.md)。PlayerCore 拥有播放状态、队列、时钟和命令顺序，插件契约不开放直接改写这些状态的入口。设备输出库及其插件实现仍在平台原型验证后确定。

manifest 声明插件及接口版本、目标平台/架构、能力、输入输出格式、参数 schema、延迟/seek 支持与权限。Host 负责版本协商、配置、启停、取消、错误映射与资源回收；主题和控制插件的故障不能直接进入音频输出回调。Client 与 Server 使用各自的宿主契约，可共享适配代码，不承诺跨运行端、平台或架构的二进制通用。

控制面使用版本化接口，覆盖能力查询、构建/配置、启动、取消、seek/reset、flush/drain、错误与释放。高频音频数据使用进程内有界二进制帧队列或有租约的缓冲，不连续通过 JSON/Base64 传送 PCM。PCM 块明确采样格式、采样率、声道数及布局、帧数、时间戳和时间基准；采样率或声道布局转换由显式节点承担。DSP 报告延迟、尾音和时间映射，PlayerCore 据此维护输出时钟与歌词位置。

后续由 Rust Client Plugin Host 管理可安装包。内置和用户原生插件统一在 Rust 宿主同一进程内加载动态库；Windows DLL、macOS dylib、Linux so 的实际支持逐 target 验证。需要 C++ 的插件采用 CMake/C++23，进程内扩展边界限版本化 C 兼容数据和不透明句柄，Rust/C++ 语言对象、异常及分配器所有权不得跨界。原生插件并无权限或故障隔离，崩溃/内存破坏可能终止 Client；C++ 异常捕获不保证拦截访问违例或其他未定义行为。安装、动态装载、重启启停与版本切换见[插件实施](implementation/plugins.md)；二进制 SDK 和机制仍待实现验证。

## 2. 原生音频链与解码后端

```text
Authorized input -> Demux -> Decode -> PCM
                 -> Client DSP -> Transition gain -> Device output
```

音频引擎独立于窗口事件循环。解码、播放时钟与设备输出由播放器管理，PCM 保留在音频通路中。本机设备输出不需要再编码。Server 已处理的效果通过播放描述进入 Client 的实际生效链；同一效果仅由指定端执行。桌面端在本机输出时处理进度跳转和切歌的淡出淡入。

Windows 的候选后端为 Windows Media Foundation（WMF）：通过 Media Source/Source Reader 读取媒体并使用可用 decoder MFT 获得 PCM；Source Reader 不承担播放器时钟或设备输出。另一条路径为随发行包内置的 FFmpeg 解复用/解码插件。实际格式覆盖取决于系统组件或 FFmpeg 构建，须经过能力探测和打开验证，不宣称任一后端支持全部格式。

可配置 `auto` / `WMF` / `FFmpeg` 及优先级。在打开失败或格式不支持时按已验证能力尝试其他后端，显示失败原因；播放中更换后端由 PlayerCore 取消旧图、清理缓冲后重建，不能混接不同解码状态。后端回退不替代来源切换，也不放宽共同协议中的续播条件。

实时输出回调不执行网络、文件读写或阻塞式插件调用；预分配音频缓冲，由非实时线程解码预取、处理控制请求和重建处理图。Host 限制缓冲和任务数量并处理背压；图更换发生在明确的块/播放边界，旧图不再使用后才回收实例及缓冲，动态库仍保留到进程退出。对于仍可运行的宿主，欠载和可检测超时报告明确状态，故障实例不再接收新任务；取消依赖插件协作，超时不能保证线程已停止。卡死实例不得复用其缓冲，排空失败保持引用并提示重启；原生崩溃可能使界面和播放器同时退出。

## 3. 可配置淡出淡入

在「账号与设置」的「播放」中可分别设置以下两项。两项均默认开启，并在本机保存。

- **跳转时淡出淡入**。播放中调整进度时，先淡出当前位置的音频，再定位并从目标位置淡入。暂停时调整进度会保持暂停。
- **切歌时淡出淡入**。手动换曲或队列自动进入下一首时，先淡出仍可播放的旧曲，再从新曲淡入。首次播放和队列播完沿用正常的开始与停止行为。

淡出与淡入各用 300 毫秒。新位置或新曲有音频可输出时才开始淡入。关闭某项后，对应操作直接切换。连续操作以最后一次选择为准。

长期插件设计预留 `fadeOutMs` 和 `fadeInMs` 参数，取值 0 表示立即完成该阶段。PlayerCore 负责编排播放时序和取消过时操作，插件只调整增益。同曲故障恢复沿用恢复流程，停止和退出可中断尚未完成的转场。

## 4. 播放协议适配

Server 会话的创建、停止、恢复与定位计划经 Rust 协议适配层发送到 `/api/v1/control` 的 `musicfs.control.v1` WebSocket。request 内 method/path 对应 `/playback/sessions` 及 seek/recover/recover-source 子路径，音频仍由 HTTP 读取器按返回的 streamUrl 获取。这些是 Server 共同 API，不是下述 Client 对外控制 API；后者的传输选择独立验证。会话、资源身份和跨源续播条件以[共同协议与行为](../../docs/architecture.md)为准。

PlayerCore 上报当前尝试、观察到的错误和曲目时间，只应用当前播放代次对应的响应。每次新尝试加载新 URL，清理旧输出缓冲；按共同协议返回的时间映射恢复，不能续接旧字节偏移。条件不满足或定位失败时呈现从头播放/取消选择，不自行猜测位置。Server 的来源重试、转码状态和缓存算法由[Server 音频处理设计](../../server/docs/playback/audio-processing.md)管理。

## 5. 主题插件

主题使用插件注册、安装、启停与版本管理；初期为声明式资源、颜色、字体和布局 token，不执行任意脚本。兼容声明绑定版本化原生主题接口，资源/token 由既定组件消费，不允许主题自行调用 播放与系统控制接口。内置默认主题使用相同契约，主题加载失败可回到默认资源；在本次已启用主题之间更换资源可即时生效，不得重启音频引擎或修改播放状态。主题插件启停或版本变更仍在重启后生效。

## 6. 外部控制 API 插件

可选 `control.api` 插件将请求转换为 PlayerCore 类型化命令：`play`、`pause`、`stop`、`prev`、`next`、`seek`、`volume`、`queue` 和 `status`，支持状态查询/订阅。UI、快捷键与 API 共用命令入口和权限检查；请求携带 `commandId`、播放代次及必要的 `expectedRevision`，避免重试和并发导致重复切歌。事件携带序号/修订，断线后可读取状态快照。

外部控制默认关闭；启用后默认仅允许本机访问，网络使用 loopback，本地 IPC 使用同等访问限制。使用独立、可撤销且限定作用范围的 token；HTTP、WebSocket 或 IPC 的选择后续验证。远程控制需显式配置监听范围、鉴权和安全传输；浏览器请求还须验证来源。

API 不提供任意代码/系统命令执行、任意文件读取或 Server/Provider 凭据读取入口；曲目引用仍经过 PlayerCore 与 Server 授权。插件安装、配置及权限管理不随普通播放控制权限开放。停用插件只保存下次启动禁用并显示“重启后生效”；当前监听和能力保持到退出，应用退出时停止接收请求并释放订阅。即时阻止某调用方使用独立 token 撤销，不把它等同于插件停用。

## 7. 验证重点与技术依据

- 验证 WMF/FFmpeg 的能力探测、错误回退、定位和输出时钟；验证所选构建的格式支持、延迟、资源占用及打包方式。
- 验证不同采样格式和 DSP 延迟处理；确认 Server 与 Client 不重复执行同一效果。
- 分别验证跳转与切歌开关的四种组合，以及暂停时跳转、连续定位、连续切歌、缓冲等待、恢复和退出时的转场取消。
- 验证正常播放任务取消后无残留音频和失效缓冲访问；插件库保留到进程退出，插件启停在重启后生效。可返回的主题/API 错误按契约处理，原生崩溃不承诺播放可继续。
- 验证共同协议的资源替换、按时间恢复和无法安全续播时的提示，歌词窗口使用正确的播放时钟。
- 验证 API 与 UI 并发操作、事件重连、本机鉴权、插件停用待重启、即时撤销 token 以及远程访问需显式授权。

技术依据：[FFmpeg 媒体处理链](https://ffmpeg.org/ffmpeg.html#Detailed-description)、[Microsoft Source Reader](https://learn.microsoft.com/en-us/windows/win32/medfound/source-reader)、[MFT](https://learn.microsoft.com/en-us/windows/win32/medfound/about-mfts)、[Media Foundation 媒体格式](https://learn.microsoft.com/en-us/windows/win32/medfound/supported-media-formats-in-media-foundation)。这些资料不替代本项目的实际平台验证。
