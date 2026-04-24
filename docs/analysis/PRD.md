# PRD: TransEcho 单向会议翻译桥接模式

## 1. 文档目的

本文档用于记录 TransEcho 从“系统音频实时同传工具”扩展为“单向会议翻译桥接工具”的产品需求、技术可行性分析、架构改造方案、实施路径、风险与验收标准。

本文档面向以下角色：

- 产品设计与需求确认
- 后端 / 音频链路开发
- 前端控制界面开发
- 测试与联调

本文档对应的目标模式为：

- 用户说中文
- 软件实时翻译为英文
- 软件将英文 TTS 输出到虚拟麦克风
- Zoom / Teams / Meet 等会议软件只接收虚拟麦克风
- 对方听到英文，不听到用户中文原声

## 2. 需求背景

当前 TransEcho 的核心能力是：

- 捕获系统音频
- 发送到火山引擎 Volcengine AST WebSocket API
- 接收源语言字幕与翻译字幕
- 可选播放翻译 TTS

当前产品更适合“听懂别人”，即：

- 听会议、直播、视频
- 将系统声音转为目标语言字幕 / 语音

当前新增需求与原有方向不同，需求聚焦于：

- 只需要“让别人听懂我”
- 不需要“听懂别人”
- 不需要系统音频采集
- 需要会议软件把翻译后的英文当作麦克风输入

## 3. 需求澄清

### 3.1 用户明确需求

用户需求已明确为以下单一目标：

1. 用户只需要让别人听懂自己，不需要听懂别人。
2. 用户说中文，对方直接听到英文。
3. 对方不应听到用户中文原声。
4. 会议软件的输入设备应当是“翻译后英文语音”的虚拟麦克风。
5. 用户使用设备为 Mac。
6. 主要会议场景为英语会议。

### 3.2 需求翻译为系统目标

系统必须提供一条新的单向音频链路：

`实体麦克风 -> 中文语音 -> 翻译服务 -> 英文 TTS -> 虚拟麦克风 -> 会议软件`

### 3.3 非目标

以下内容不在本次目标范围内：

- 不要求翻译并播放别人说的话
- 不要求继续保留当前系统音频字幕功能作为主流程
- 不要求首版支持自动识别所有会议软件
- 不要求首版自研 macOS 虚拟音频驱动
- 不要求首版支持多人本地混音
- 不要求首版支持闭环回声消除、噪声抑制、声纹保留等高级音频能力

## 4. 产品定义

### 4.1 产品名称

建议将新增能力命名为：

- `Mic Bridge`
- 中文名可称为：`单向会议翻译桥接`

### 4.2 用户价值

用户在英文会议里无需直接说英文，而是：

- 用中文自然表达
- 软件自动翻成英文
- 会议中的其他参与者直接听到英文语音

这比“字幕翻译工具”更接近真实的跨语言发言工具。

### 4.3 核心价值主张

- 降低用户英语口语门槛
- 保持会议发言节奏
- 不把中文原声暴露给会议中的其他参与者
- 最大程度复用现有翻译与 TTS 基础能力

## 5. 当前项目现状分析

### 5.1 当前项目已有能力

#### 前端

- 主界面：[`src/routes/+page.svelte`](../../src/routes/+page.svelte)
- 设置页面：[`src/routes/settings/+page.svelte`](../../src/routes/settings/+page.svelte)
- 本地化：[`src/lib/i18n.ts`](../../src/lib/i18n.ts)

#### 后端

- Tauri 启动与命令注册：[`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs)
- 会话编排与主状态机：[`src-tauri/src/commands.rs`](../../src-tauri/src/commands.rs)
- 协议客户端：[`src-tauri/src/transport/client.rs`](../../src-tauri/src/transport/client.rs)
- 协议编解码：[`src-tauri/src/transport/codec.rs`](../../src-tauri/src/transport/codec.rs)
- 输入重采样：[`src-tauri/src/audio/resample.rs`](../../src-tauri/src/audio/resample.rs)
- TTS 播放：[`src-tauri/src/audio/playback.rs`](../../src-tauri/src/audio/playback.rs)

#### 平台采集

- macOS 系统音频采集：[`src-tauri/src/audio/capture_macos.rs`](../../src-tauri/src/audio/capture_macos.rs)
- Windows 系统回放采集：[`src-tauri/src/audio/capture_windows.rs`](../../src-tauri/src/audio/capture_windows.rs)

### 5.2 当前项目不满足新增需求的原因

当前项目不满足新增需求的关键原因包括：

1. 输入源错误  
   当前 macOS 主输入链路基于 ScreenCaptureKit 捕获系统音频，而不是实体麦克风。

2. 输出方向错误  
   当前 TTS 默认输出到系统扬声器，不是虚拟麦克风设备。

3. 会议路由未建立  
   当前没有“软件输出 -> 虚拟音频设备 -> 会议软件麦克风”的完整音频路由。

4. 权限模型不匹配  
   当前项目主要需要屏幕录制权限，而新增模式首先需要麦克风权限。

5. 状态机以“听懂别人”为主  
   目前的静音检测、自动暂停、回声抑制等逻辑主要围绕“系统音频采集 + 本地 TTS 回放”设计，不完全适合“麦克风桥接”场景。

## 6. 可行性结论

### 6.1 总结论

当前项目可以二次开发实现目标需求。

前提条件：

- 不自研虚拟麦克风驱动作为第一阶段方案
- 首版依赖现成 macOS 虚拟音频设备，例如 BlackHole 2ch 或 Loopback
- 复用现有 Volcengine AST 的 `s2s` 能力输出英文 TTS

### 6.2 为什么可行

项目已具备以下关键基础：

- 现成的流式 WebSocket 会话管理
- 现成的 protobuf 协议封装
- 现成的 `s2s` 模式与 TTS 音频消费逻辑
- 现成的输入重采样链路
- 现成的 Tauri 前后端通信机制

因此新增需求主要是“替换输入源 + 替换输出终点 + 增加设备控制”，不是从零开始。

### 6.3 第一阶段不建议自研虚拟驱动的原因

自研 macOS 虚拟音频驱动存在以下问题：

- CoreAudio HAL 驱动复杂度高
- 分发、签名、兼容性成本高
- 会显著拉长交付周期
- 对单用户自用场景收益不足

结论：

- 第一阶段应当使用外部虚拟音频设备
- 推荐优先支持 BlackHole 2ch / Loopback

## 7. 外部方案与竞品参考

### 7.1 sokuji 的参考价值

参考仓库：

- [kizuna-ai-lab/sokuji](https://github.com/kizuna-ai-lab/sokuji)

其可参考点包括：

- 产品定位：翻译用户自己的声音
- 产品链路：翻译后的音频送入虚拟麦克风
- 使用场景：Zoom / Meet / Teams 等会议软件

不建议直接复用其代码的原因：

- 技术栈不同，sokuji 偏 Electron / WebAudio / WebRTC 体系
- 当前项目是 Tauri + Rust 音频链路
- sokuji 许可证为 AGPL-3.0，直接代码复用存在许可证风险

结论：

- 可参考产品设计与链路思路
- 不建议直接引用代码实现

### 7.2 BlackHole 的角色

参考项目：

- [ExistentialAudio/BlackHole](https://github.com/ExistentialAudio/BlackHole)

BlackHole 的定位：

- macOS 虚拟音频 loopback driver
- 可将一个应用输出的音频作为另一个应用输入

在本项目中的角色：

- 作为英文 TTS 输出的目标设备
- 作为 Zoom / Teams / Meet 的麦克风输入设备

### 7.3 Loopback 的角色

Loopback 是商业产品，但对首版方案有很高参考价值：

- 可以创建虚拟音频设备
- 可以将应用输出组合为虚拟输入
- 用户配置体验更友好

本项目第一阶段不需要与 Loopback 做 API 集成，只需支持“将输出定向到指定输出设备”。

## 8. 产品范围

### 8.1 In Scope

- 新增单向桥接模式 `Mic Bridge`
- 从实体麦克风采集中文语音
- 发送中文语音到 Volcengine AST
- 接收英文 TTS 音频
- 将英文 TTS 输出到指定虚拟音频设备
- 允许会议软件将该虚拟设备选作麦克风
- 默认不输出中文原声
- 提供输入设备、输出设备、语言和音色设置
- 提供启动 / 停止 / 静音等基础控制

### 8.2 Out of Scope

- 首版不做双向翻译
- 首版不做自研虚拟音频驱动
- 首版不做完整音频增强算法
- 首版不做声纹克隆保真优化
- 首版不做自动安装 BlackHole
- 首版不做会议软件插件级集成

## 9. 用户场景

### 9.1 标准会议场景

1. 用户提前安装 BlackHole 2ch
2. 用户启动 TransEcho
3. 用户在设置中选择：
   - 输入设备：Mac 实体麦克风
   - 输出设备：BlackHole 2ch
   - 源语言：中文
   - 目标语言：英文
4. 用户在 Zoom / Teams 中将麦克风切换为 BlackHole 2ch
5. 用户开始发言
6. 对方听到英文，不听到中文

### 9.2 自测场景

1. 用户打开 QuickTime 录音
2. 录音输入选择 BlackHole 2ch
3. 用户用中文说话
4. QuickTime 录到的是英文 TTS
5. 若录音中没有中文原声，则链路正确

## 10. 核心设计原则

1. 优先复用现有翻译链路
2. 新增能力尽量与旧系统音频模式解耦
3. 输入与输出设备必须可选
4. 默认不做中文原声旁路
5. 首版优先稳定性，不优先极限低延迟
6. 保留字幕与状态信息作为调试辅助

## 11. 功能需求

### 11.1 模式切换

系统需要支持两种运行模式：

- `system_caption`
- `mic_bridge`

定义：

- `system_caption`：现有模式，捕获系统音频并显示字幕 / 可选同传
- `mic_bridge`：新模式，捕获实体麦克风并输出翻译后的英文到虚拟设备

### 11.2 音频输入设备选择

用户必须能够选择实体麦克风输入设备。

要求：

- 枚举当前可用输入设备
- 支持默认设备与指定设备
- 设备名在 UI 中可见
- 启动时校验设备是否可用

### 11.3 音频输出设备选择

用户必须能够选择英文 TTS 的输出设备。

要求：

- 枚举可用输出设备
- 优先展示 BlackHole / Loopback 等虚拟设备
- 启动前必须明确输出设备
- 若输出设备不可用，启动失败并提示错误

### 11.4 发言翻译

当用户用中文发言时，系统需要：

- 采集麦克风输入
- 将音频重采样为 API 所需格式
- 发给 AST `s2s`
- 接收英文字幕与英文 TTS

### 11.5 虚拟麦克风输出

系统需要将英文 TTS 音频输出到指定虚拟设备。

要求：

- 默认只输出翻译后英文
- 默认不输出中文原声
- 输出格式应兼容虚拟设备常见采样率和声道配置
- 对会议软件表现为标准输入设备

### 11.6 启停控制

用户必须能：

- 启动桥接
- 停止桥接
- 看到当前运行状态

### 11.7 静音控制

用户应能手动静音。

静音定义：

- 采集链不中断
- 但不再发送有效用户语音，或发送静音帧
- 输出侧不再产生英文语音

### 11.8 本地监听

本地监听为可选能力。

默认行为：

- 关闭

可选行为：

- 将英文 TTS 同时输出到第二设备，例如扬声器 / 耳机

### 11.9 调试字幕

首版建议保留最近一条：

- 中文原文
- 英文译文
- 状态信息

用途：

- 方便联调
- 帮助判断延迟、断句和错误

## 12. 非功能需求

### 12.1 延迟

目标：

- 正常网络下，用户说中文到对方听到英文，延迟控制在可接受范围内
- 首版目标可接受区间：约 1 到 2 秒级

### 12.2 稳定性

目标：

- 连续运行 30 分钟以上不崩溃
- 设备热插拔后能够报错或恢复，而不是静默失败

### 12.3 可观测性

系统应能输出日志：

- 设备选择
- 连接状态
- 输入采样率
- 输出采样率
- TTS chunk 大小
- 关键错误

### 12.4 兼容性

首版平台：

- macOS

会议软件优先联调对象：

- Zoom
- Microsoft Teams
- Google Meet

## 13. 技术方案总览

### 13.1 高层数据流

```text
实体麦克风
  -> 麦克风采集
  -> 输入重采样 (16k mono i16)
  -> Volcengine AST s2s
  -> 英文 TTS PCM (24k mono f32)
  -> 输出重采样 / 声道适配
  -> 指定输出设备 (BlackHole / Loopback)
  -> 会议软件作为麦克风输入
```

### 13.2 设计原则

- 输入与输出分离
- 旧模式不破坏
- 新模式独立命令与状态
- 设备枚举与设备选择显式化
- 输出设备可配置，不绑定默认扬声器

## 14. 后端技术设计

### 14.1 建议新增模块

新增文件建议如下：

- `src-tauri/src/audio/types.rs`
- `src-tauri/src/audio/capture_input.rs`
- `src-tauri/src/audio/device.rs`
- `src-tauri/src/audio/output_router.rs`
- `src-tauri/src/audio/output_resample.rs`

#### audio/types.rs

职责：

- 统一音频帧结构
- 统一设备描述结构

建议定义：

```rust
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

pub struct AudioDeviceList {
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
}
```

#### capture_input.rs

职责：

- 使用 `cpal` 捕获实体麦克风输入
- 支持默认设备和按名称选择设备

建议接口：

```rust
pub async fn start_input_capture(
    device_name: Option<String>,
    buffer_size: usize,
) -> Result<(mpsc::Receiver<AudioFrame>, CaptureHandle), Box<dyn std::error::Error>>
```

#### device.rs

职责：

- 枚举输入输出设备
- 获取默认设备
- 按名称查找设备

建议接口：

```rust
pub fn list_audio_devices() -> Result<AudioDeviceList, Box<dyn std::error::Error>>;
pub fn find_input_device(name: &str) -> Result<cpal::Device, Box<dyn std::error::Error>>;
pub fn find_output_device(name: &str) -> Result<cpal::Device, Box<dyn std::error::Error>>;
```

#### output_router.rs

职责：

- 打开指定输出设备
- 将英文 TTS 音频推送到设备输出流
- 可选镜像到本地监听设备

建议接口：

```rust
pub struct OutputConfig {
    pub primary_device_name: String,
    pub enable_monitor: bool,
    pub monitor_device_name: Option<String>,
    pub volume: f32,
}

pub struct OutputRouter { ... }

impl OutputRouter {
    pub fn new(config: OutputConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>;
    pub fn push_pcm_24k_mono_f32(&self, samples: &[f32]);
}
```

#### output_resample.rs

职责：

- 将 AST 返回的 24k mono f32 PCM 转换到输出设备所需格式
- 例如：
  - 24k mono -> 48k stereo
  - 24k mono -> 44.1k stereo

### 14.2 现有模块改造建议

#### Cargo.toml

文件：

- [`src-tauri/Cargo.toml`](../../src-tauri/Cargo.toml)

建议：

- 将 `cpal` 从 Windows 专属依赖调整为跨平台可用依赖，或新增 macOS target 依赖
- 如果输出层继续使用 `rodio` 作为高层封装，可保留
- 若输出设备控制需要更强能力，建议直接走 `cpal`

建议方向：

- `cpal` 用于输入采集与指定设备输出
- `rodio` 可逐步弱化，不再依赖默认输出设备

#### audio/mod.rs

文件：

- [`src-tauri/src/audio/mod.rs`](../../src-tauri/src/audio/mod.rs)

建议新增模块导出：

- `pub mod capture_input;`
- `pub mod device;`
- `pub mod output_router;`
- `pub mod output_resample;`
- `pub mod types;`

#### playback.rs

文件：

- [`src-tauri/src/audio/playback.rs`](../../src-tauri/src/audio/playback.rs)

当前职责：

- 将英文 TTS 输出到系统默认扬声器

建议改造：

- 将默认扬声器播放逻辑重构为“可配置输出设备路由”
- `TtsHandle::new()` 接收输出配置
- `play_pcm_bytes()` 解码后交给 `OutputRouter`

建议保留：

- `play_pcm_bytes()` 的字节到 `f32` PCM 解码逻辑

建议替换：

- `OutputStream::try_default()` 相关默认输出路径

#### resample.rs

文件：

- [`src-tauri/src/audio/resample.rs`](../../src-tauri/src/audio/resample.rs)

建议：

- 保留现有输入重采样器
- 可考虑补充输出重采样器
- 若不想拆文件，也可在同文件下增加第二个 resampler 结构体

#### commands.rs

文件：

- [`src-tauri/src/commands.rs`](../../src-tauri/src/commands.rs)

这是改造核心文件。

建议新增命令：

```rust
#[tauri::command]
pub async fn list_audio_devices() -> Result<AudioDeviceList, String>

#[tauri::command]
pub async fn start_mic_bridge(
    app: AppHandle,
    on_subtitle: Channel<SubtitleEvent>,
    api_key: String,
    input_device_name: String,
    output_device_name: String,
    source_language: String,
    target_language: String,
    speaker_id: String,
    enable_monitor: bool,
    monitor_device_name: Option<String>,
    hot_words: Vec<String>,
    glossary: HashMap<String, String>,
    correct_words: String,
) -> Result<(), String>

#[tauri::command]
pub async fn stop_mic_bridge(app: AppHandle) -> Result<(), String>
```

建议实现原则：

- 不要破坏现有 `start_interpretation`
- 新模式走新的命令和新的状态字段
- `AppState` 可从单 `stop_tx` 扩展为按模式区分的 stop handle

建议新状态字段：

```rust
pub struct AppState {
    pub system_session_stop_tx: Mutex<Option<mpsc::Sender<()>>>,
    pub mic_bridge_stop_tx: Mutex<Option<mpsc::Sender<()>>>,
}
```

#### lib.rs

文件：

- [`src-tauri/src/lib.rs`](../../src-tauri/src/lib.rs)

建议：

- 注册新增命令：
  - `list_audio_devices`
  - `start_mic_bridge`
  - `stop_mic_bridge`

### 14.3 Mic Bridge 运行时主流程

建议主流程：

1. 校验当前没有已运行的 `mic_bridge`
2. 读取输入设备
3. 读取输出设备
4. 建立 Volcengine AST `s2s` 连接
5. 启动麦克风采集
6. 初始化输入重采样器
7. 初始化输出路由器
8. 启动统一 tokio 任务
9. 持续执行：
   - 接收麦克风音频
   - 转换为 16k mono i16
   - 发送到 AST
   - 接收英文字幕和英文 TTS
   - 将英文 TTS 推给 OutputRouter
10. 直到用户停止或异常终止

### 14.4 自动暂停策略

现有模式有 60 秒静音自动断开，适合节省 token。

但对 `mic_bridge` 模式，不建议默认启用自动断连。

原因：

- 开会发言是间歇性的
- 自动断线会导致首句恢复延迟
- “发言即时性”比“节省 token”更重要

建议：

- `mic_bridge` 首版默认保持连接
- 仅在用户主动停止时断开

### 14.5 原声抑制策略

目标不是通过软件做复杂“消音”，而是通过设备路由保证中文原声不进入会议。

原则：

- 会议软件输入只能选虚拟设备
- 实体麦克风不要直接交给会议软件
- 应用默认不把中文原声旁路到输出设备

这是满足“对方听不到中文原声”的关键。

## 15. 前端技术设计

### 15.1 主界面改造

文件：

- [`src/routes/+page.svelte`](../../src/routes/+page.svelte)

建议新增或调整 UI 字段：

- 模式选择：`System Caption` / `Mic Bridge`
- 输入设备下拉框
- 输出设备下拉框
- 源语言下拉框
- 目标语言下拉框
- 音色下拉或快捷选择
- 本地监听开关
- 本地监听设备下拉框
- 状态文本
- 启动 / 停止按钮
- 最近一句原文 / 译文调试区域

### 15.2 设置页改造

文件：

- [`src/routes/settings/+page.svelte`](../../src/routes/settings/+page.svelte)

建议保存新增字段：

- `mode`
- `input_device_name`
- `output_device_name`
- `monitor_enabled`
- `monitor_device_name`
- `source_lang`
- `target_lang`
- `speaker_id`

### 15.3 状态文案

文件：

- [`src/lib/i18n.ts`](../../src/lib/i18n.ts)

建议新增状态键：

- `bridge_ready`
- `bridge_connecting`
- `bridge_live`
- `bridge_muted`
- `bridge_stopped`
- `input_device_missing`
- `output_device_missing`
- `virtual_device_recommended`
- `bridge_error`

## 16. 配置设计

建议新增持久化配置项：

```text
mode = "mic_bridge"
input_device_name = "MacBook Pro Microphone"
output_device_name = "BlackHole 2ch"
monitor_enabled = false
monitor_device_name = ""
source_lang = "zh"
target_lang = "en"
speaker_id = "zh_female_xiaoai_uranus_bigtts"
```

配置存储仍使用当前 `@tauri-apps/plugin-store` 即可。

## 17. macOS 权限与系统配置

### 17.1 麦克风权限

新增模式需要麦克风权限。

要求：

- App bundle 中配置 `NSMicrophoneUsageDescription`

建议文案：

`TransEcho needs microphone access to capture your speech and translate it into the virtual meeting microphone.`

### 17.2 屏幕录制权限

`mic_bridge` 模式不依赖屏幕录制权限。

保留原因：

- 若同时保留旧 `system_caption` 模式，则仍需要

### 17.3 虚拟设备安装指引

首版不内置安装程序，但需要在 UI 或文档里给出明确指引：

1. 安装 BlackHole 2ch
2. 将会议软件麦克风切到 BlackHole 2ch
3. 将 TransEcho 输出设备切到 BlackHole 2ch
4. 将 TransEcho 输入设备切到实体麦克风

## 18. 详细实施路线

### 阶段 1：POC 跑通

目标：

- 中文说话
- 英文 TTS 进 BlackHole
- QuickTime / Zoom 能录到或听到英文

任务：

1. 增加输入设备枚举
2. 增加麦克风采集模块
3. 增加指定输出设备写入
4. 新增 `start_mic_bridge`
5. 前端新增模式与设备选择

阶段 1 验收：

- QuickTime 录音输入选 `BlackHole 2ch`
- 用户说中文
- QuickTime 录到英文
- 录音中无明显中文原声

### 阶段 2：稳定化

目标：

- 可连续会议使用

任务：

1. 设备丢失异常处理
2. 输出 ring buffer
3. 输出重采样
4. 静音控制
5. 更细致的状态提示

阶段 2 验收：

- 连续运行 30 分钟稳定
- 输出设备被拔掉时有清晰错误提示
- 停止后能完全停音

### 阶段 3：可用性增强

目标：

- 适合真实会议长期使用

任务：

1. 本地监听
2. 快捷键静音 / push-to-talk
3. 启动前设备检查
4. 新手指引

## 19. 验收标准

以下标准全部满足时，可认为该需求完成：

1. 用户可以从 UI 选择实体麦克风输入设备。
2. 用户可以从 UI 选择虚拟音频输出设备。
3. 用户能用中文发言，系统能实时生成英文 TTS。
4. 英文 TTS 能进入 BlackHole / Loopback。
5. Zoom / Teams / Meet 麦克风切到虚拟设备后，对方能听到英文。
6. 对方不会直接听到用户中文原声。
7. 用户可以停止桥接，会议软件随即失去该英文音频输入。
8. 下次启动软件时，设备选择能被正确恢复。

## 20. 风险与约束

### 20.1 最大风险

最大风险不是翻译 API，而是 macOS 音频路由：

- 输出设备选择
- 采样率兼容
- 会议软件设备识别
- 虚拟设备稳定性

### 20.2 许可证风险

需要注意：

- `sokuji` 为 AGPL-3.0，不应直接复用代码
- BlackHole 的分发和内嵌使用需要关注其许可证说明

### 20.3 用户侧配置风险

若用户把会议软件麦克风选成实体麦克风而不是 BlackHole，则：

- 中文原声仍会进会议
- 产品目标无法达成

因此必须在 UI / 文档中强调设备选择步骤。

## 21. Open Questions

以下问题在正式开发前应进一步确认：

1. 输出设备是否固定优先支持 BlackHole 2ch？
2. 是否需要同时支持 Loopback 配置说明？
3. 首版是否保留字幕区，还是将其下沉为调试面板？
4. 首版是否需要手动静音按钮？
5. 是否需要 push-to-talk？
6. 是否需要同时支持“本地监听英文”？

## 22. 开发建议总结

建议按照以下优先级推进：

1. 不改坏现有系统音频模式
2. 新增 `mic_bridge` 独立模式
3. 先打通“实体麦克风 -> 英文 TTS -> BlackHole”
4. 先用 QuickTime 做链路自测
5. 再接 Zoom / Teams / Meet 联调
6. 最后再考虑监听、快捷键、复杂稳定性增强

## 23. 一句话结论

TransEcho 完全可以扩展为“中文说话 -> 英文虚拟麦克风输出”的单向会议桥接工具。最佳路径是在保留现有翻译基础设施的前提下，新增麦克风采集与可配置输出设备能力，并依赖 BlackHole / Loopback 作为首版虚拟麦克风方案。
