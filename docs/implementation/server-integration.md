# Server 接入与原生界面

2026-11-20 已接入原生 HTTP 传输，使用 Rust 管理会话及 WebSocket 播放控制。当前覆盖范围见[桌面验证记录](../validation/desktop.md)。阶段见[桌面实施计划](../implementation.md)。

## 1. 当前契约与原生播放前置项

共同语义以[总体架构](../../../docs/architecture.md)为准，精确接口属于 [Server OpenAPI](../../../server/docs/contracts/openapi.yaml)、[HTTP Schema](../../../server/docs/contracts/http.schema.json)及 [WS 控制通道](../../../server/docs/contracts/control-channel.md)。当前库和 Web 的实现范围不能从长期架构中的目标路由推断；表中列出完整适配要求。

| 项目 | 当前事实 | 桌面实施要求 |
| --- | --- | --- |
| 登录 | 引导 Bearer + 允许 Origin 换取 Cookie 与 CSRF；没有原生 access/refresh token API | Rust 管理私有会话并验证真实握手，不臆造 token 接口 |
| 播放能力 | `capabilities` 使用 `htmlAudio: true` 或 `nativeHttp: true`，二者互斥，`formats` 必填 | Rust 声明 `nativeHttp: true`，按实际可解码能力填写 MIME |
| 输出传输 | `outputProfile.transport` 回显协商的 `nativeHttp` 或 `htmlAudio` | 核对 `nativeHttp` 和实际输出格式，经 Rust HTTP 读取器加载授权音频 |
| 交付模式 | 当前 Schema 接受 `proxy` / `processed`，流地址是同源相对路径 | 首版消费这两类；`direct` 保留为后续协议增量 |
| 控制与通知 | `/control`、`musicfs.control.v1`；播放请求、心跳与 `capabilities.changed`，本机另有 `lifecycle.changed` | Rust 统一连接、鉴权、关联响应、重连并重新获取授权能力；其他变化按已交付查询契约处理 |
| 本地生命周期 | WS request 显式申请/续期/释放，Cookie/Origin/首帧 CSRF 和实际 loopback 检查 | 仅对实际 client-managed 实例维护租约；ping 不续租，不接管 standalone |

服务端原生传输已在[创建逻辑](../../../server/src/playback/create.go)实现。C0 用授权样本验证本机解码，C1 以 Rust 适配器联调播放创建、格式不匹配及实际 HTTP 读取。真实上游账号和在线交付能力按来源另外验收。

## 2. 原生鉴权与网络边界

### 2.1 会话所有权

原生登录表单把输入的引导凭据交给 Rust 登录任务，随即清空输入。该凭据仅用于建立会话。当前只持有本次运行的会话，不将令牌写入地址、偏好或日志。

Rust Server Adapter 为每个连接/账号管理独立 Cookie jar、CSRF 与身份摘要。`POST /auth/session` 使用引导 Bearer 及允许的 Origin；后续 HTTP 请求使用私有 Cookie，状态修改附带 Origin 和 `X-CSRF-Token`。WS 升级发送 Cookie、允许的 Origin 及 `musicfs.control.v1`，升级后 5 秒内首帧发送 `{type:"auth",csrfToken}`，收到 ready 才提交控制请求。每次心跳和命令由 Server 复核登录。不把引导 Bearer 附加到所有请求，不向界面返回 Cookie/CSRF 或平台凭据。现有 Cookie 的 HttpOnly、SameSite、Secure、路径、过期及注销语义仍适用，原生 Cookie 实现不能跳过 TLS/作用域校验。

适配器从用户提供的 Server 地址确定 Origin，后续登录、WS 握手和修改请求使用同一 Origin。部署必须显式允许这个来源，授权由 Cookie 与 CSRF 共同完成。

连接时检查 Server 的产品、API 版本、实例标识和就绪状态。以下分别列出本机测试和远程部署的配置要求。

| 模式 | 示例配置 | Server 前提 |
| --- | --- | --- |
| 本地 | API `http://127.0.0.1:8080/api/v1`；Origin `http://127.0.0.1:8080` | 显式 loopback 监听、`allowLoopbackHttp=true`、允许列表包含该 Origin |
| 远程 | API `https://music.example.test/api/v1`；Origin `https://music.example.test` | 管理员显式允许该 Origin，正常 TLS 证书校验；域名仅为文档示例 |

当前[配置校验](../../../server/src/core/config/validate.go)只接受相应的 HTTP(S) Origin。远程连接使用 HTTPS，本机回环地址可按部署设置使用 HTTP。

账号切换、会话失效或退出时增加连接代次，取消在途查询/播放、失效内容句柄和清空用户缓存。有效会话的 Cookie/CSRF 保留到第 3 节必要收尾请求完成或有界失败后再清除，不在发出注销前丢弃。会话到期不得无限尝试旧凭据；重新授权由明确流程触发。退出平台账号与退出 MusicFS 会话是不同操作，沿用 Server 的账号代次、绑定与撤销语义。

### 2.2 音频与媒体对象

当前 `streamUrl` 按固定 Server 基址解析并验证同源及允许路径。Rust 读取器使用对应会话，处理 Range、取消和资源版本；不把完整音频先下载为 Blob 作为播放前提。重定向默认拒绝，确需支持时重新检查目标，禁止把 Cookie、Bearer 或 CSRF 转发给不同来源。

未来 direct 若交付，必须新增兼容声明，并使用不带 Server 凭据的独立请求上下文；短期地址如何授权按 Server 契约定义。Client 不直接获取 Provider 密钥、NAS 凭据或内部插件数据通道。

歌词、封面等受鉴权对象由 Rust 适配器读取。对象标识、大小和可接受的媒体类型在展示前校验。账号切换时丢弃当前页面与媒体状态，损坏的资源仅影响对应显示。

Cookie jar 属于当前会话。图片、歌词与音频分别按各自的协议读取。

## 3. 事件、并发和生命周期

当前实现为每次创建或关闭播放会话建立短时 WS 控制连接，校验子协议及 ready 的实例身份后发送命令，并匹配响应 ID。长连接订阅、心跳、能力刷新和重连是后续目标：重新订阅后读取权威快照，使用连接代次隔离迟到结果。

本机连接接收 `lifecycle.changed` 初始快照及后续状态，停机事件尽力送达；断线后仍须重新发现实例，不能假定一定收到最后通知。request 使用独立的响应关联 id 和业务 idempotencyKey，正文遵循 Server Schema。单条 WS 同时承载播放与租约：Server 为两类命令各维护容量 8 的独立队列与工作协程，播放保持串行，续租不等待慢播放命令；响应必须按 id 关联，不能假定跨队列顺序。通知独立产生，共享容量 32 的出站队列。HTTP 控制端点只作管理入口，不在 WS 断线后自动回退。音频保持独立 HTTP 流。

服务端每条命令从读取并受理时开始计时，排队与执行共限 15 秒；排队过期不会进入业务。Rust 适配须同时约束等待和迟到结果的清理窗口，不能把客户端取消等待当作服务端撤销。

Client 内部 PlayerCore 快照/事件是另一套版本化协议；Server 能力事件不能代替本机队列、设备或播放状态。页面请求绑定连接、账号、查询和请求代次；迟到响应不得跨账号显示。分页沿用不透明游标和固定修订，不将未知能力、空数据与读取失败混为一类。

播放控制遵循主计划的串行 seek/recover 和 attempt 更新；通道不自动重放已发送修改，客户端不得从 WS 断线或取消等待推断服务端操作已经撤销。计划中的 Rust 适配须区分未发送的网络失败与发送后未知结果；业务层显式重试时沿用原键，不能确认当前尝试则提示重新播放。跨来源时间证据、处理配置和不可续播提示保持共同语义。

租约以 [Server 生命周期](../../../server/docs/operations/lifecycle.md)为准：只有真实 loopback 上的 client-managed 实例接受租约，默认有效 30 秒、建议每 10 秒通过 WS request 显式续租；创建请求带实例 ID/客户端 ID/幂等键，续租按上次成功序号加一，重试使用相同序号。连接 ping 不续租，断线宽限只由原租约期限决定，过期不能复活。续租任务由 Rust 驱动，独立于窗口事件循环和可见性。

正常退出并注销的顺序为：停止新操作 → 有界停止播放/插件任务 → 取消并等待续租任务结束 → 使用仍有效的会话发起注销 → 清理本地凭据与资源并退出。Server 的[会话撤销回调](../../../server/src/app/bootstrap/composition.go)会一并撤销该会话的播放和所有租约，注销后无需再发 release。最后租约 release 会立即触发 draining，因此不能将“先 release 再保证注销成功”写成正常路径。

仅在保留会话、主动脱离托管实例的流程中单独 release，此后不保证实例仍接受其他请求。注销/释放响应丢失时按现有接口语义有界处理，不无限等待；无法确认服务端注销则只报告本地退出/未确认撤销，并依靠租约期限回收。休眠恢复先重新检查身份、实例与租约，过期租约不可复活。remote 不申请本机租约，external-local 不改变实例原始归属。

## 4. 桌面与 Web 的设计对应

桌面采用[共享 UI 设计](../../../docs/ui-design.md)中的页面组织。侧栏用于切换内容，底栏持续显示当前播放状态。所有控件和视图由 Rust 原生渲染；网络、声音输出、图片解码和窗口生命周期分别由桌面模块管理。

| 交互 | 原生实现 |
| --- | --- |
| 曲库、歌手、专辑、流派与搜索 | 使用 Server 查询及分页游标，后台请求完成后刷新当前页面 |
| 收藏、歌单、最近播放 | 读取账号集合及已发布的歌单快照；编辑歌单按规则修订提交 |
| 播放和桌面歌词 | PlayerCore 管理队列与时钟；独立窗口读取同一播放器的状态 |
| 作品包与推荐 | 根据 Server 声明的类型、分组和来源呈现；推荐预览独立于个人歌单修改 |

协议字段由 Server 契约定义。桌面适配器把经验证的 HTTP 与 WS 响应转换为 Rust 数据类型。

托管 Server、用户安装插件、设备选择和平台账号管理属于后续目标。桌面从 Server 读取来源的名称和译名，按账号上下文展示。

桌面作品库从 Server 读取内容类型声明，将提供 `library` 的类型按 `library.order` 放在“曲目列表”后，使用声明的名称和图标。打开入口时以对应 `contentType` 查询作品，例如 CD 专辑使用 `cd_album`，ASMR 使用 `asmr`，电台使用 `radio`。曲目列表、分类数量与播放全部均直接消费 Server 的普通曲目查询结果。

Server 的 `showInTrackList` 控制作品音轨是否进入普通曲库。桌面从相应查询读取结果；作品编辑表单属于后续目标。

普通曲库读取 `/library/tracks`。播放前按曲目 ID 查询最新可用来源，指定的来源保持固定。作品包的音轨则直接使用授权的来源曲目标识。

收藏与音乐的最近播放使用特殊歌单。桌面历史页按内容类型的 `library.separateHistory` 声明追加标签，分别读取 `/me/content-history/{type}`。播放器确认音频开始播放后报告来源、播放会话和尝试编号，归属由服务端确认。

## 5. C1/C2 联合验收清单

- 新原生协商模型由 Server 定义并通过 Schema/实现/SDK 对齐检查；旧 Web 请求和输出格式回归通过。
- 真实 Rust 登录、恢复、到期、退出、Origin 不允许、CSRF 错误和 TLS 错误均有明确结果；没有虚构的原生 token 端点。
- 原生音频及封面/歌词通过各自授权读取，跨用户、旧句柄、过期尝试、跨源重定向不能泄露内容或凭据。
- UI 重建、多窗口、账号切换、WS 重连和查询晚响应不创建重复播放器、不显示上一用户数据；握手超时、心跳超时、登录撤销和未知结果原键重试均有明确状态。
- proxy/processed 真实发声；byteRange/timeRestart、连续 seek、结果未知重试、取消旧代次符合共同状态语义。
- 托管模式续租在主窗口隐藏或 窗口重建后继续；注销撤销会话租约，单独 release 与正常注销分开验收，独立/远程 Server 不被接管或停止。

本节列出了完整验收目标。已执行的命令、目标平台和限制见[桌面验证记录](../validation/desktop.md)。
