# 桌面端实现方案：React + Tauri

状态：实施依据 v0.2；2026-10-11。已选择 TypeScript + React、Tauri 2 与 Rust 原生核心；尚无可运行桌面实现，本文所有阶段均待实施。原生插件统一与 Rust 宿主同进程运行；选定路线不等于三平台兼容、重启恢复或音频性能已经通过验证。

## 1. 范围与文档分工

目标平台为 Windows、macOS、Linux。Client 提供桌面交互、原生播放、可安装的客户端插件、桌面歌词及受控的本地 Server 启动；音乐库、来源解析、Provider/Scraper 与账号业务仍属于 Server。

| 文档 | 内容 |
| --- | --- |
| [Client 架构](architecture.md) | 模块职责与状态所有权 |
| [音频与插件设计](audio-processing.md) | 音频链、转场、插件能力的长期约束 |
| [插件能力与宿主边界](plugin-capabilities.md) | 可替换角色、固定宿主职责、输出/分析/系统控制扩展 |
| [Server 接入与界面复用](implementation/server-integration.md) | 当前协议缺口、原生鉴权、播放协商、React 复用边界 |
| [插件实施](implementation/plugins.md) | 用户安装包、单进程动态库、C ABI、重启启停与版本切换 |
| [平台与验收](implementation/platforms-validation.md) | 三平台能力、打包、测试与证据 |
| [桌面组件图](diagrams/desktop-components.puml) | React、Tauri、PlayerCore、音频及插件数据通道 |

[共同架构](../../docs/architecture.md)定义跨端语义；[Server 契约](../../server/docs/contracts/README.md)拥有精确接口。本文提出的桌面命令、插件字段和原生播放协商均须在对应实施阶段落地，不能当作现有 API 使用。本次只编写文档，不建立空实现或改变 Server/Web 行为。

## 2. 技术决策与验证边界

| 层次 | 选择 | 尚需验证 |
| --- | --- | --- |
| 界面 | TypeScript 严格模式、React、Vite；CSS Modules + CSS 变量；hash 路由 | 与现有 Web 组件的实际兼容、三种系统 WebView、输入法与长列表 |
| 桌面宿主 | Tauri 2；本地打包的前端静态资源 | 窗口、托盘、快捷键、安装包与权限配置 |
| 应用核心 | Rust；PlayerCore、Server 适配器、插件管理与生命周期 | 并发、异步取消、状态恢复与原生 FFI 边界 |
| 解复用与解码 | 优先验证 FFmpeg 库适配；Windows 可增加 WMF 适配 | 构建、格式覆盖、seek、部署依赖；不默认启用 FFmpeg 全部功能 |
| 设备输出 | `audio.output` 插件契约；Rust/CPAL 为首轮内置适配候选 | 设备枚举、格式协商、热插拔、输出帧时钟与各平台后端；库尚未最终选定 |
| 本机 DSP | 原生音频图；Rust 实现或 C ABI 接入 CMake/C++23 插件 | 有界缓冲、延迟报告、切图与实例资源回收 |
| 用户插件 | MusicFS 插件 SDK/宿主，原生包在同一宿主进程内动态装载 | DLL/dylib/so 逐 target 验证；同进程不提供原生权限或故障隔离 |

Node.js/npm 只用于前端构建，不默认作为安装后的运行时依赖。Rust 不要求所有音频代码重写；需要 C++ 的插件统一使用 CMake/C++23，既有 C 库保持原有实现，通过版本化 C ABI 接入。Tauri 的 IPC 与窗口工具不承担解码、DSP 或播放时钟。共同规则见[原生插件规范](../../docs/native-plugins.md)。

C0 记录确切 Node/npm、React、Vite、Rust 工具链、Tauri JS/Rust 配套版本、CMake、C++23 编译器/标准库、FFmpeg 构建配置和目标三元组；C1 提交锁文件及复现步骤。不得把 Web 的锁文件直接复制成已验证的桌面环境，也不使用不固定版本的依赖作为发布依据。

## 3. 工程布局

以下是计划布局，按阶段逐步创建。源文件放 `src/`，入口放 `src/app/`，测试统一放根 `tests/`；单个源文件尽量不超过 400 行。

```text
client/
  package.json / package-lock.json
  Cargo.toml / Cargo.lock / rust-toolchain.toml
  index.html / vite.config.ts / tsconfig.json
  src/
    app/                  # React main.tsx；Tauri main.rs/lib.rs、Cargo.toml、build.rs
      tauri.conf.json
      capabilities/
    ui/                   # 外壳、页面、展示组件、主题、歌词视图
    bridge/               # TypeScript 命令与事件适配、运行时校验
    contracts/            # Rust/TS 边界 DTO、Schema 及受控生成入口
    player-core/          # Rust crate：唯一播放状态、命令串行与时钟
    server-adapter/       # Rust crate：HTTP、会话、SSE、播放协商
    audio-engine/         # Rust crate：读取、解码、PCM 图、设备输出
    plugin-host/          # Rust crate：安装、注册、C ABI 与动态库引用管理
    platform/             # Rust crate：窗口、凭据、启动器与系统集成
    plugins/              # 按角色拆分的内置/示例实现；C++ 使用 CMake/C++23
  sdk/client-plugin/      # C ABI 头文件、角色接口、Schema 与 CMake 接入配置
  tests/
    unit/                 # React、桥接和独立领域逻辑
    native/               # Rust 集成、FFI、协议与插件生命周期
    e2e/                  # 真实 Server、桌面窗口及播放
    fixtures/             # 可分发测试音频和测试插件
  docs/
    implementation.md
    implementation/
    diagrams/
```

Rust 使用根 Cargo workspace，各领域 crate 的 `[lib].path` 指向本目录的 `lib.rs`；应用 `[lib]` / `[[bin]]` 显式指向 `src/app/` 内入口，避免再生成一层业务 `src/`。Rust 集成测试通过各 manifest 的 `[[test]]` 与相对路径指向根 `tests/native/`；不把测试散入源目录。

Tauri 项目目录采用 `src/app/`，不同于脚手架默认 `src-tauri/`。前端脚本在仓库根运行，桌面脚本显式以 `src/app/` 为工作目录调用本地锁定的 Tauri CLI；前端构建 hook 的工作目录显式回到仓库根，`frontendDist` 指向 `../../dist`。Vite 的入口仍为根 `index.html`，引用 `src/app/main.tsx`；开发地址使用固定 loopback 端口，端口占用立即报错。C0 必须验证这组自定义路径的开发与生产构建，不能只验证默认模板。

构建产物、缓存和运行时插件目录不入 Git。SDK 及界面共享代码以后按版本发布或受控同步，不能通过运行时跨子模块源码路径形成隐式依赖。

## 4. 运行结构与状态所有权

Tauri 原生主进程组合 Rust 模块。PlayerCore、Server 连接、租约续期、插件管理和设备输出不依赖任何 React 组件是否挂载。WebView 只呈现界面；主窗口、迷你播放器、歌词窗口均订阅同一核心。

内置和用户原生插件均在 Tauri 原生宿主同一进程内装载 DLL/dylib/so；不采用插件子进程，也不将音频引擎拆为独立进程。通过任务、线程与有界队列避免音频处理阻塞 UI；这种调度分离不提供原生崩溃隔离。插件启停、更新及回滚统一重启后生效，当前能力保持到退出；不提供插件卸载，已加载库保持引用到进程退出。插件故障可能终止 Client，卡死不能强制停止线程，实例排空失败保持引用，见[插件分册](implementation/plugins.md)。Tauri/WebView 自身的系统辅助进程不承载 MusicFS 原生插件。

### 4.1 React 与原生桥接

桥接暴露按领域收敛的类型化命令，例如播放、读取快照、来源查询、插件管理和窗口操作；这些是待实现的内部命令，不是 Server 路由。不提供任意 URL 请求、任意进程执行或任意动态库路径的通用入口。

- 命令包含请求标识、必要的 `commandId`、播放代次与预期修订；Rust 再次验证结构、权限、当前连接与状态。返回“已接受”与“已完成”分开表达，队列接收成功不等于已经发声。
- 核心发布带 `coreInstanceId`、可靠状态序号和状态修订的快照/事件；快照状态与序号在同一次原子读取中生成。初始化等待订阅建立确认并暂存事件，再读取快照，只应用该实例中比快照新的连续状态事件。缺口或重启时按相同步骤重取；重取绑定订阅代次，旧响应丢弃，暂存超出上限则重建订阅，不能无限缓存。
- React 用外部 store 和 `useSyncExternalStore` 读取快照。组件局部状态只保存输入草稿、选择和展开状态，不另建可写的播放队列/音量/活动曲目事实源。
- 事件按类别有界排队：播放错误和状态变化使用可靠状态序号；频繁位置进度采用独立、允许跳号的进度序列并允许合并，不用进度跳号判定状态缺口。进度绑定核心实例、播放代次和时钟版本，旧曲进度不得覆盖新状态。重连、窗口重建或页面切换不能丢失当前播放状态；前端卸载必须取消自己的订阅。
- TypeScript 类型不替代运行时校验。桥接 DTO 以版本化 Schema/生成输入为依据，Rust 与 TS 的序列化样本双向校验，时间统一毫秒，未知值保留 null。

### 4.2 前端权限与内容

按窗口标签声明 capabilities：歌词窗口只接收必要歌词/时钟与受限窗口命令，主窗口和设置入口按领域授权。应用自定义命令必须加入 Tauri 的命令权限生成/清单并显式分配，不能假定仅写窗口 capabilities 就约束了全部自定义命令。

生产仅加载随包静态资源，配置 CSP 和资源读取范围；普通网页与平台授权页不获得桌面桥接权限。主题资源、歌词及媒体元数据作为数据处理，不允许注入任意 HTML/脚本。账号授权跳转按 Server 插件挑战流程处理，回调验证挑战身份和有效期；不向第三方页面注入 MusicFS 凭据。

主窗口关闭行为区分“隐藏到后台”与“退出应用”。默认退出；用户启用后台驻留且有已验证的恢复入口时隐藏主窗口，继续播放并保持续租。若系统入口不可用，不能把用户关进不可见且无法退出的进程。显式退出按[会话收尾顺序](implementation/server-integration.md#3-事件并发和生命周期)停止输出和续租，释放插件资源并通过会话注销撤销对应租约。

## 5. 原生音频实施

### 5.1 数据与时钟

原生 HTTP 读取器消费 Server 返回的固定表示，向解复用/解码器提供有界输入；凭据由读取器管理，不作为插件配置或前端 URL 参数。解码得到 PCM 后经显式采样格式/声道转换、Client DSP、转场增益与设备输出。Server 已执行的效果通过 `effectiveProcessing` 排除，避免重复处理。

优先验证 FFmpeg 的库级接口与受控 I/O；简单执行 `ffmpeg` 命令生成整首文件不能作为原生流式播放或可插拔解码链的完成证据。WMF 只补充 Windows，不能成为 macOS/Linux 的共同前置条件。实际采用版本、编译选项和分发依赖写入验收记录。

音频数据留在同一原生进程内的有界二进制队列/带租约缓冲，React 与 Tauri 命令通道只传控制和摘要。设备回调使用预分配缓冲，不做网络、磁盘、锁等待、动态分配或阻塞插件调用；解码、DSP 和控制处理在非实时任务中运行，可视化使用可丢帧旁支。欠载时输出受控静音并报告状态，缓冲不能无限增长，长期失败不能以无限静音掩盖。

PlayerCore 的播放位置来源于实际输出帧及明确的设备/DSP 延迟模型，结合 `positionOriginMs` 和 Server 时间映射转为曲目时间。暂停、欠载、seek、设备重开和采样率变化必须重设或冻结对应时钟；不能用 React 定时器或下载字节数充当播放进度。歌词窗口可在短时间内插值绘制，但需定期按核心快照校正，断开后停止猜测。

### 5.2 控制与恢复

- `play/pause/stop/seek/next/prev/volume`、系统媒体键和 `control.api` 都进入同一命令入口。快速 A/B/A 切歌废弃旧代次，晚到的网络和 PCM 输出均不得恢复旧曲。
- 同一会话 seek/recover 串行，最多一个请求在途；连续定位只保留最新待执行目标。在途返回先确认其 attemptId，再发下一请求，遵守[共同控制语义](../../docs/implementation.md#32-播放描述与客户端状态)。
- byteRange 验证资源身份、长度和服务端实际能力；timeRestart 加载新的表示和曲目起点。新表示清理旧缓冲，不沿用旧字节偏移。
- 网络结果不明使用原幂等键有界重试；取消本地请求不代表服务端撤销。无法确认当前尝试时停止自动控制并提示重新播放。
- 解码后端回退、设备切换、同源刷新和跨源恢复分别处理。跨源续播只有 Server 返回满足资产版本与时间证据的决定后执行，无法安全恢复时由用户选择重播。
- 纯本机 DSP、输出设备或本机解码图改变推进 `graphGeneration` 并清理旧图；不因此要求 Server 创建新 attempt。只有需要改变服务端来源、资产、表示或处理输出时才按协议重新协商；已有输入是否可重读/定位仍以当前表示能力为准。
- 转场沿用顺序“淡出旧曲 → 切换 → 新 PCM 就绪后淡入”，首版默认关闭；不把缓冲等待计入淡入，也不把同曲恢复伪装成换曲。

设备拔出默认暂停并清空失效设备缓冲，提供重新选择输出的入口；自动切换到其他设备需独立偏好和验证。未验证独占、bit-perfect、ASIO 或所有采样率之前，不在设置中声称支持。

## 6. 分阶段实施与完成门槛

所有阶段当前均为待实施；C0 验证材料是技术原型，不等于可交付桌面客户端。

| 阶段 | 交付内容 | 完成门槛 |
| --- | --- | --- |
| C0 技术原型 | 三平台 React/Tauri 构建与窗口、测试音频设备输出、CMake/C++23 DSP 动态库及 C ABI、协作取消/实例回收/重启启停与恢复、自定义目录验证 | 每个平台记录实际运行；输出库/FFmpeg 构建有候选证据；定位协议缺口并形成 Server 变更清单；接受同进程崩溃边界 |
| C1 工程与会话 | 固定工具链、锁文件、桥接 DTO/权限、Rust 会话存储、来源列表；Server 原生播放契约增量 | 真实 Cookie/CSRF 握手及退出；浏览器旧能力回归通过；原生能力不能冒报 htmlAudio |
| C2 首次原生播放 | remote/external-local 连接；真实库点播、暂停、停止、队列、byteRange 和实际时钟 | Windows/macOS/Linux 分别实际听音、非零定位与快速切歌；隐藏/重建界面不改变核心状态，显式退出正确停止 |
| C3 处理与动态插件 | processed/timeRestart、恢复、DSP/转场；解码/DSP/输出包安装、启动装载、待重启启停/更新/回滚；外部控制 API | 兼容新包无需重编译，重启后从磁盘装载；当前/期望状态一致呈现；实例回收与故障恢复通过，库始终保留到退出；外控鉴权/撤销通过 |
| C4 桌面呈现 | 主题、歌词/迷你窗口、托盘、媒体键、输出设置、多屏缩放；可视化/系统控制角色扩展 | 功能矩阵逐项记录三平台实际结果；新角色从包加载并符合数据/回调约束；不支持项明确呈现并有可用退路 |
| C5 桌面业务 | 复用已验收 React 展示组件；来源账号、搜索、推荐、歌单、刮削与配置页面 | 逐项核对 Server 实际能力、账号隔离和修订；不复制浏览器播放器、不过度声明平台播放能力 |
| C6 本地协作 | managed-local 发现/互斥/启动、租约、多客户端、崩溃回收及升级 | 独立 Server 不被接管；remote 不启动本地进程；升级失败保留恢复入口 |
| C7 交付验收 | 各目标平台安装、原生依赖、插件目录、签名配置、升级与回归证据 | 安装制品在目标系统实测；无凭据入包；完成所宣称的平台/功能矩阵，发布仍需单独授权 |

主链为 C0 → C1 → C2 → C3 → C7；C4/C5 在桥接稳定后可并行，C6 在原生会话可用后实施。最小可播放版本是 C2；带可安装插件的首版必须通过 C3 与对应 C7，完整桌面交付覆盖 C4—C6。某个平台受限只能缩小实际支持声明并记录未完成项，不能替它勾选通过。

## 7. 验收记录与后续交付

实施时在本仓库建立逐阶段验证记录，保存 Client/Server/Web 基线提交、操作系统与架构、依赖、测试命令、人工步骤、期望/实际结果、限制与证据位置。测试材料不得包含私人音乐、账号或凭据；程序日志及测试输出使用英文，文档与用户说明使用中文。

文档、源码和实现提交归 Client 仓库；Server 契约增量归 Server，Web 共享组件提取归 Web。更新实现版本时在主仓库同步对应子模块指针，不能将三个独立仓库改造成主仓库源码目录。

官方依据核对日期为 2026-10-10：[Tauri/Vite 集成](https://v2.tauri.app/start/frontend/vite/)、[进程模型](https://v2.tauri.app/concept/process-model/)、[调用 Rust](https://v2.tauri.app/develop/calling-rust/)、[权限与边界](https://v2.tauri.app/security/capabilities/)、[CPAL 文档](https://docs.rs/cpal/latest/cpal/)、[FFmpeg 库文档](https://ffmpeg.org/doxygen/trunk/index.html)。库文档说明可用机制，不替代 MusicFS 的平台与音频验证。
