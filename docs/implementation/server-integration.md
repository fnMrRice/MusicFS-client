# Server 接入与 React 界面复用

状态：实施计划；2026-10-10。已核对当前 Server 契约与实现；尚未以 Rust/Tauri 客户端联调。本文列出可沿用机制、必须扩展的协议和界面迁移边界，阶段见[桌面实施计划](../implementation.md)。

## 1. 当前契约与原生播放前置项

共同语义以[总体架构](../../../docs/architecture.md)为准，精确接口属于 [Server OpenAPI](../../../server/docs/contracts/openapi.yaml)及 [HTTP Schema](../../../server/docs/contracts/http.schema.json)。当前库和 Web 的实现范围不能从长期架构中的目标路由推断。

| 项目 | 当前事实 | 桌面实施要求 |
| --- | --- | --- |
| 登录 | 引导 Bearer + 允许 Origin 换取 Cookie 与 CSRF；没有原生 access/refresh token API | Rust 管理私有会话并验证真实握手，不臆造 token 接口 |
| 播放能力 | `Capabilities` 必填 `htmlAudio=true`；创建逻辑拒绝 false | Server 先增加向后兼容的原生播放器能力模型，Client 不冒报浏览器能力 |
| 输出传输 | `OutputProfile.transport` 当前固定为 `htmlAudio` | Server 明确原生 HTTP 读取/解码的传输语义，并同步 Schema、SDK 与选择逻辑 |
| 交付模式 | 当前 Schema 接受 `proxy` / `processed`，流地址是同源相对路径 | 首版消费这两类；`direct` 保留为后续协议增量 |
| SSE | 当前只有 `capabilities.changed` | Rust 统一订阅并重新获取授权能力；其他变化按已交付查询/轮询契约处理 |
| 本地生命周期 | Cookie、真实 loopback TCP；修改另验 Origin/CSRF | 仅对实际 client-managed 实例维护租约，不接管 standalone |

能力限制同时存在于 Schema 与[创建逻辑](../../../server/src/playback/create.go)，不能仅在客户端重新解释 `htmlAudio` 就宣称原生播放已兼容。C0 可以用授权测试样本验证本机解码/输出，真实 Server 原生播放交付必须先完成 C1 的兼容增量。

Server 增量的需求为：区分浏览器媒体元素与原生播放器；接收原生实际可用的编码/容器/采样格式和定位能力；返回双方理解的输出传输描述；未知或不支持能力给出明确错误。具体字段、枚举和版本策略由 Server 仓库设计与维护，本文件不预先发布新的线上字段。

该增量须同时更新 OpenAPI、JSON Schema、SDK/样本、请求校验、输出选择及兼容记录，并验证：现有 Web 的请求/响应继续可用；新桌面面对旧 Server 明确提示不兼容；原生能力缺失或格式不匹配时不返回无法处理的输出。桌面自身可解码不代表上游授权、Apple Music 裸流或任一在线来源已可用。

## 2. 原生鉴权与网络边界

### 2.1 会话所有权

React 登录表单可将用户输入的引导凭据提交给专用 Rust 登录命令，随后清空输入；该凭据没有一次使用后自动失效的语义。凭据不进入 URL、localStorage、通用事件、遥测或普通日志。保存登录信息仅在用户选择且平台安全存储可用时启用；安全存储不可用时仅维持本次内存会话。

Rust Server Adapter 为每个连接/账号管理独立 Cookie jar、CSRF 与身份摘要。`POST /auth/session` 使用引导 Bearer 及允许的 Origin；后续请求使用私有 Cookie，状态修改附带 Origin 和 `X-CSRF-Token`。不把引导 Bearer 附加到所有请求，不向 WebView 返回 Cookie/CSRF 或平台凭据。现有 Cookie 的 HttpOnly、SameSite、Secure、路径、过期及注销语义仍适用，原生 Cookie 实现不能跳过 TLS/作用域校验。

连接配置包含 Server API 基址与配套 `desktopOrigin`。该 Origin 必须由可信部署配置明确允许，不能让普通界面在每次请求时指定任意值，也不枚举允许项试探登录。Rust 可以发送 HTTP(S) Origin，但 Origin 本身不是客户端身份，实际认证仍由会话完成。

代码核查得到以下可行候选，均待 C1 握手验证：

| 模式 | 示例配置 | Server 前提 |
| --- | --- | --- |
| 本地 | API `http://127.0.0.1:8080/api/v1`；Origin `http://127.0.0.1:8080` | 显式 loopback 监听、`allowLoopbackHttp=true`、允许列表包含该 Origin |
| 远程 | API `https://music.example.test/api/v1`；Origin `https://music.example.test` | 管理员显式允许该 Origin，正常 TLS 证书校验；域名仅为文档示例 |

当前[配置校验](../../../server/src/core/config/validate.go)只接受规定的 HTTP(S) Origin，不能直接假定 `tauri://` 或 `http://tauri.localhost` 已可用。Rust 原生网络请求不需要通过通配 CORS 绕过 WebView 限制；不修改 Server 浏览器会话保护来适配桌面。部署不满足既有规则时显示配置要求，协议调整另由 Server 验证。

账号切换、会话失效或退出时增加连接代次，取消在途查询/播放、失效内容句柄和清空用户缓存。有效会话的 Cookie/CSRF 保留到第 3 节必要收尾请求完成或有界失败后再清除，不在发出注销前丢弃。会话到期不得无限尝试旧凭据；重新授权由明确流程触发。退出平台账号与退出 MusicFS 会话是不同操作，沿用 Server 的账号代次、绑定与撤销语义。

### 2.2 音频与媒体对象

当前 `streamUrl` 按固定 Server 基址解析并验证同源及允许路径。Rust 读取器使用对应会话，处理 Range、取消和资源版本；不把完整音频先下载为 Blob 作为播放前提。重定向默认拒绝，确需支持时重新检查目标，禁止把 Cookie、Bearer 或 CSRF 转发给不同来源。

未来 direct 若交付，必须新增兼容声明，并使用不带 Server 凭据的独立请求上下文；短期地址如何授权按 Server 契约定义。Client 不直接获取 Provider 密钥、NAS 凭据或内部插件数据通道。

歌词、封面等受鉴权对象由 Rust 读取。WebView 使用限当前用户、窗口、对象版本及生命周期的本机资源句柄/受控协议，不直接把 Server 鉴权路径当作 `<img>` 可访问地址。句柄不可指定任意文件路径；验证资源类型、大小、内容及缓存配额，退出/账号切换即撤销，损坏资源只影响对应显示。

原生 Cookie jar 不与 WebView Cookie 存储自动同步，也不通过注入 Cookie 复用 Web 组件。图片/歌词引用和播放地址分开管理；不向主题或外部控制 API 返回私有流授权。

## 3. 事件、并发和生命周期

每个活动连接由 Rust 维护一条 SSE 订阅，多窗口共享结果。收到 `capabilities.changed`、首次连接或重连后重新读取 Server 权威能力；不能把通知当作完整事件日志或假定支持 Last-Event-ID 重放。能力刷新绑定连接/会话代次，采用单在途读取与脏标记：读取期间再次收到变化，则完成后再读取一次，只发布当前代次结果；变化后的旧快照不能用于新敏感操作，Server 仍逐请求鉴权。revision 是不透明字符串，不按字典序比较新旧。鉴权失败进入会话恢复，其他断线有界退避，退出时关闭订阅。

Client 内部 PlayerCore 快照/事件是另一套版本化协议；Server 能力事件不能代替本机队列、设备或播放状态。页面请求绑定连接、账号、查询和请求代次；迟到响应不得跨账号显示。分页沿用不透明游标和固定修订，不将未知能力、空数据与读取失败混为一类。

播放控制遵循主计划的串行 seek/recover、原键重试和 attempt 更新；客户端不得从 HTTP 取消推断服务端操作已经撤销。跨来源时间证据、处理配置和不可续播提示保持共同语义。

租约以 [Server 生命周期](../../../server/docs/operations/lifecycle.md)为准：只有真实 loopback 上的 client-managed 实例接受租约，默认有效 30 秒、建议每 10 秒续租；创建请求带实例 ID/客户端 ID/幂等键，续租按上次成功序号加一，重试使用相同序号。续租任务由 Rust 驱动，不能依赖 React 定时器或窗口可见性。

正常退出并注销的顺序为：停止新操作 → 有界停止播放/插件任务 → 取消并等待续租任务结束 → 使用仍有效的会话发起注销 → 清理本地凭据与资源并退出。Server 的[会话撤销回调](../../../server/src/app/bootstrap/composition.go)会一并撤销该会话的播放和所有租约，注销后无需再发 release。最后租约 release 会立即触发 draining，因此不能将“先 release 再保证注销成功”写成正常路径。

仅在保留会话、主动脱离托管实例的流程中单独 release，此后不保证实例仍接受其他请求。注销/释放响应丢失时按现有接口语义有界处理，不无限等待；无法确认服务端注销则只报告本地退出/未确认撤销，并依靠租约期限回收。休眠恢复先重新检查身份、实例与租约，过期租约不可复活。remote 不申请本机租约，external-local 不改变实例原始归属。

## 4. Web 复用策略

现有 Web 已采用 React/TypeScript。复用以[共享 UI 设计](../../../docs/ui-design.md)、[Web React 方案](../../../web/docs/react-development.md)及实际源码为依据；其历史“不建设桌面”的范围不限制本次 Client 计划。

| 对象 | 复用方式 |
| --- | --- |
| 图标、主题变量、展示组件、曲目表和页面布局 | 审计平台假设后抽出展示层；沿用中文交互、动态加载、来源身份与真实能力显示 |
| 不可变 store、查询键、游标/修订逻辑与测试样本 | 复用纯逻辑或按相同规则移植；保留运行时校验和账号隔离 |
| React 服务注入 | 抽成服务端口，Web 实现浏览器适配，Client 实现类型化 Rust 桥接；组件不直接导入 Tauri |
| Web `ApiClient` / EventSource | 替换为 Rust HTTP/SSE；不直接复用同源 fetch 和 JS CSRF 存储 |
| 浏览器 audio adapter / 播放控制器 | 由 Rust PlayerCore 与原生引擎替换；迁移代次、幂等和恢复测试，不运行两套可写核心 |
| 歌词/封面读取与 `MediaPanel` | 改为服务端口和受控资源句柄，不能依赖 WebView 自动携带原生 Cookie |
| `pagehide` 停播、`window.location.origin` | 改为桌面关闭策略与真实 Server 连接摘要；页面销毁不直接停播或注销 |
| MusicKit 等外部授权脚本 | 平台授权、CSP、回调和权限隔离另验；不直接加载进拥有桌面权限的主界面 |

首轮先复用设计与可独立测试的纯组件；需要持续共享代码时，由 Web 所有者提取独立入口并固定版本，记录源提交、导出接口、资产许可及两端测试。Client 不运行远程 Web 站点作为本地桌面首页，不直接导入整个 `web/src/app`，不复制两套播放状态规则后长期各自修改。

协议 DTO 可依据 Server 的版本化 Schema 分别生成 TS/Rust 类型；生成器、输入版本和输出位置写入记录，Server 私有数据库结构不进入 Client。Tauri IPC DTO 是客户端内部契约，不能覆盖 Server SDK。

桌面增加窗口控制、迷你播放器、插件页、输出设备与歌词设置；通过能力接口呈现，不能反向要求 Web 显示无效的桌面按钮。来源名称/译名、音乐来源分组、账号歌单分类和管理员目录权限遵循最新共同架构，不在桌面维护平台 ID 到显示名称的固定映射。

## 5. C1/C2 联合验收清单

- 新原生协商模型由 Server 定义并通过 Schema/实现/SDK 对齐检查；旧 Web 请求和输出格式回归通过。
- 真实 Rust 登录、恢复、到期、退出、Origin 不允许、CSRF 错误和 TLS 错误均有明确结果；没有虚构的原生 token 端点。
- 原生音频及封面/歌词通过各自授权读取，跨用户、旧句柄、过期尝试、跨源重定向不能泄露内容或凭据。
- UI 重建、多窗口、账号切换、SSE 重连和查询晚响应不创建重复播放器、不显示上一用户数据。
- proxy/processed 真实发声；byteRange/timeRestart、连续 seek、结果未知重试、取消旧代次符合共同状态语义。
- 托管模式续租在主窗口隐藏或 WebView 重建后继续；注销撤销会话租约，单独 release 与正常注销分开验收，独立/远程 Server 不被接管或停止。

以上均为后续验收要求，本次代码阅读只能证明当前实现约束，不能替代真实客户端联调。
