# 桌面播放器验证

## 2026-11-23 曲库、设置与播放

以下结果来自 Windows x64 原生窗口、Rust 测试、系统凭据管理器和独立运行的 MusicFS Server。涉及曲库写入的用例使用临时数据库、专用账号与生成的测试音频。播放设置截图记录了曲库验收时的界面，当前淡变设置可分别调整切歌、暂停和跳转时长。

| 场景 | 验证方式和结果 |
| --- | --- |
| Windows 窗口 | 已保存的登录自动恢复后，曲库显示五首曲目、多张不同的封面及无封面占位图。验证勾选及清除曲目、打开重复识别与来源版本窗口，来源版本提供「播放此版本」。[曲目列表截图](../screenshots/windows-track-list.png) |
| 设置与退出 | 在实际窗口打开七组设置，切换播放配置、本地音效及插件管理。显式退出后返回登录窗口，提供「在此设备上保持登录」。[播放配置截图](../screenshots/windows-playback-settings.png) · [登录截图](../screenshots/windows-remember-login.png) |
| 系统凭据 | 使用本机凭据管理器验证默认仅本次登录、勾选后重建应用状态自动连接、会话失效后重新登录以及退出时删除凭据。实例或账号改变时恢复被拒绝，普通偏好文件中没有登录令牌。 |
| 自动与固定的来源版本 | 分别在两个隔离实例中读写收藏和歌单的逻辑曲目引用与固定来源引用。修改录音的默认来源后，自动项跟随新的默认值，固定项仍指向原选版本。 |
| 本地音乐源 | 在隔离服务端选择允许访问的目录，添加、改名、写入导入深度和歌词选项、跟踪自动扫描至完成、读回统计，保存监听参数并移除目录。 |
| 文件夹浏览 | 扫描含 104 份 WAV 的子目录，读取 100 条和 4 条两页，校验全部来源 ID 唯一。文件名搜索命中一条，用第二页选中来源创建播放会话，核对返回的来源 ID 并关闭。 |
| 插件管理 | 在 egui 测试帧加载真实服务端插件和实例，验证必需数据库标记、用途及 ID 筛选、关闭后重新启用本地来源实例。另以单元测试验证声明式配置的必填项、数值范围与步长、文本长度、选项及布尔字段。 |
| 重复录音 | 两份测试 WAV 触发服务端哈希任务，生成 100 分候选并按分数合并。同一幂等键重试返回原操作 ID；解除关联、排除、撤销排除、手动重新关联和按曲名与作者批量处理均通过。 |
| 专辑内曲目合并 | 在测试专辑中创建两首曲目，验证过期修订被拒，合并后读回自选标题、别名与两位艺人的署名，同键重试保留原结果。 |
| 本机音效与输出 | 使用合成 PCM 检查增益、两类均衡、动态压缩、混响、自动音量平衡、低频声道降混、环绕扩展和播放中参数更新；验证双声道上限下的布局降级及 100 毫秒淡变。请求构造测试核对本机 DSP 与强制服务端转码标志。 |
| 强制转码与本机音效 | 在 Windows 桌面播放同一份双声道 48 kHz WAV，本机增益设为 −12 dB。开启强制转码前收到原流 PCM，开启后由 FFmpeg 6.1.1 提供 320 kbps MP3。两次实际播放均收到服务端开始回执，服务端记录 `dspExecution=client`，本机播放时钟均超过一秒。 |
| 偏好恢复 | 在窗口中静音、保存一个增益节点并切换到英语。关闭再启动后恢复原账号、英语、静音及音效链，取消静音回到原来的 80% 音量。 |

## 2026-11-20 首次 Windows 窗口与安装包验收

Windows x64 使用 Rust 1.100.0（MSVC）、eframe/egui 0.31.1 和 rodio 0.21.1。下列截图记录本次实机窗口。

| 场景 | 验证方式和结果 |
| --- | --- |
| 生产界面和原生应用 | 从仓库根目录执行 `cargo build --release --locked -p musicfs-desktop` 生成 Windows x64 应用。窗口和控件由 egui/eframe 原生绘制。 |
| Windows 界面操作 | 通过 Windows UI Automation 操作实际应用窗口，登录后读到五首扫描曲目，完成搜索、收藏及读回、新建歌单和发布快照，打开推荐来源与 CD 作品库，并从曲表播放和暂停 WAV。[实机曲库截图](../screenshots/windows-library.png) |
| 服务端连接 | 使用从 MusicFS Server 项目构建的隔离实例、随机管理员令牌和动态回环端口。真实登录、曲库分页接口、搜索、收藏、歌单创建及追加、幂等重试、发布快照并打开歌单、注销均通过。 |
| 多页歌单 | 在隔离实例创建 51 个歌单，通过 egui 的辅助功能接口点击“读取更多目标歌单”。验证编辑器读到第 2 页，刷新列表后仍能打开该页的歌单并读取已发布的曲目。 |
| 原生播放 | Rust 播放器通过控制通道协商 `nativeHttp`，按授权读取固定表示。已验证 WAV、FLAC、VBR MP3 的设备播放；测试实例配置 FFmpeg/ffprobe 提供编码与容器信息。独立测试覆盖 WAV 解码、非零定位及清空队列时丢弃迟到的加载结果。 |
| 作品库 | 使用独立 ASMR 作品验证目录导入、真实包树、按来源点播、授权读取 WAV 和按作品归档播放历史。 |
| 歌词 | Rust 解析测试覆盖多标签、同时间译文、时间偏移及纯文本。 |
| 辅助窗口 | Windows 为[迷你播放器](../screenshots/windows-mini.png)和[桌面歌词](../screenshots/windows-lyrics.png)分别创建独立窗口。暂停在 0:02 时，迷你窗口显示 0:02 / 0:06，歌词窗口显示该位置的当前行与下一行。关闭歌词窗口后主程序继续运行。 |
| 返回主窗口 | egui 控件测试点击“主窗口”，验证向主窗口发送还原与聚焦命令，并关闭迷你窗口。Windows 前台鼠标点击仍待实测。 |
| Windows 安装包 | NSIS 生成当前用户安装包。解包检验可执行文件与 Rust release 的 SHA-256 一致，并确认包含 `vcruntime140.dll`。 |

## 复验

```sh
cargo build --release --locked -p musicfs-desktop
cargo test --locked --workspace
```

服务端专项测试使用独立实例，默认跳过。将 `MUSICFS_TEST_ENDPOINT` 设为该实例的回环地址、`MUSICFS_TEST_TOKEN` 设为测试账号令牌，另以 `MUSICFS_DESKTOP_CONFIG_DIR` 指向临时偏好目录。准备好测试所需的音乐后，可按场景复验。

| 测试 | 准备和命令 |
| --- | --- |
| 服务端基本往返 | 扫描可播放的 WAV。运行 `cargo test --locked -p musicfs-server-adapter --test session_and_stream isolated_server_round_trip -- --ignored`，测试将修改收藏并创建歌单。 |
| 来源版本与曲目引用 | [两项服务端用例](../../src/server-adapter/tests/catalog_references.rs)分别使用两份不同的已扫描音频和一份可播放音频。每次使用全新的隔离实例，执行 `cargo test --locked -p musicfs-server-adapter --test catalog_references <测试名> -- --ignored`。 |
| 多页歌单 | 运行 `cargo test --locked -p musicfs-desktop opens_playlist_from_second_page -- --ignored`，测试将创建 51 个歌单。 |
| 保持登录 | 使用测试用系统用户和可用的凭据库，运行 `cargo test --locked -p musicfs-desktop isolated_remember_current_account_and_forget_on_logout -- --ignored`。测试使用独立的凭据命名空间并自行清理。 |
| 文件夹分页 | 按[夹具约定](../../tests/native/folders.rs)在根 `local` 下扫描 104 份不同的 WAV，并设置 `MUSICFS_TEST_FOLDERS=isolated`。运行 `cargo test --locked -p musicfs-desktop isolated_folder_paging_and_fixed_source_playback -- --ignored`。 |
| 音乐源与专辑 | 分别准备[本地目录夹具](../../tests/native/settings_sources.rs)或[专辑夹具](../../tests/native/album_merge.rs)，并设置对应的 `MUSICFS_TEST_SOURCES=isolated` 或 `MUSICFS_TEST_ALBUM_MERGE=isolated`。用 `cargo test --locked -p musicfs-desktop <测试名> -- --ignored` 运行 `isolated_sources_directory_scan_and_watch_round_trip` 或 `isolated_album_track_merge_round_trip`。 |

## 平台范围

Windows x64 已验证原生窗口、默认设备播放和服务端 FFmpeg 转码。5.1/7.1 实体扬声器听测、干净 Windows 系统安装与卸载尚待验证。第三方真实账号的扫码、Apple MusicKit 授权和个性化推荐仍需平台账号与授权环境。macOS、Linux 的安装、窗口、设备播放与系统凭据后端仍待各平台实测。设备独占模式、可安装的桌面原生插件和托管 Server 启动属于长期设计。
