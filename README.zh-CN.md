# mzk

无界面的音乐点播 CLI：用一条命令搜索 Jamendo，按字符匹配与加权随机选择歌曲，退出 CLI 后由 mpv 继续播放。

[English](README.md) · [发行版本](https://github.com/styayur/mzk/releases) · [验证记录](docs/VERIFICATION.md)

**0.1.0-alpha.1 · Alpha · Windows / Linux x86-64。** 已实现离线逻辑与模拟 IPC 生命周期测试；真实 mpv 播放、扬声器输出尚未验证。实际执行结果以[验证记录](docs/VERIFICATION.md)为准。

## 功能

- 唯一业务命令 `mzk p <text>`，另有标准帮助和版本。所有文字按普通 Unicode 字符串处理，不使用 LLM 或意图识别。
- NFKC 与小写规范化，按字素簇计算字符重合、连续片段、编辑距离和子序列匹配。
- 过滤低分候选后加权随机抽取；首次使用完整原文搜索，必要时最多追加一次连续片段搜索。
- 仅管理一个无界面的 mpv，验证本地 IPC 进程身份，确认加载及播放状态后才报告成功，歌曲结束后退出。
- 无数据库、历史、下载、遥测、账号、GUI、TUI 或独立守护进程。

## 安装与首次运行

自行安装可信的 [mpv](https://mpv.io/installation/) **0.40 或更新版本**，需支持音频输出及 JSON IPC。程序不会下载 mpv，发行包也不包含它。将其目录加入绝对 PATH，或通过 `MZK_MPV` 指定可信可执行文件的绝对路径。Windows 程序未签名；CI 发行目标为 Windows MSVC 和 Linux GNU x86-64，其他系统暂不支持。

从[发行页](https://github.com/styayur/mzk/releases)下载压缩包，与相邻校验文件比较 SHA-256 后解压，将目录加入 PATH。在 [Jamendo 开发者门户](https://devportal.jamendo.com/)登记自己的应用、接受条款，并设置客户端 ID。当前音源集成仅面向**个人非商业收听**；MIT 仅覆盖程序，不覆盖 API 或音乐。使用前阅读[音源条件](docs/PROVIDER.md)。

```powershell
$env:JAMENDO_CLIENT_ID = 'your_registered_client_id'
# mpv 不在 PATH 时可设置：
$env:MZK_MPV = 'C:\Tools\mpv\mpv.exe'
mzk p "Clair de lune"
mzk p "segmentation fault"
mzk p "我希望明天是晴天"
```

```sh
export JAMENDO_CLIENT_ID='your_registered_client_id'
mzk p 'Clair de lune'
```

示例均为普通查询，不保证曲库有对应内容。整个输入须引用为一个参数，元字符遵循 Shell 引用规则。搜索帮助类字符串可用 `mzk p -- '--help'`。空白输入、超过 256 字素簇或 4096 UTF-8 字节会被拒绝。`.env.example` 只是说明，程序不会自动读取 `.env`。

成功时输出真实曲目信息及必要署名。以下仅为格式示意，**不是实测播放结果**：

```text
playing Artist — Track
Jamendo · https://www.jamendo.com/track/TRACK_ID
License: https://creativecommons.org/licenses/by/4.0/ (unmodified stream)
```

退出码：0 成功，1 运行错误，2 参数错误。`MZK_DIAGNOSTICS=1` 可在 stderr 显示阶段耗时、请求次数及是否复用；不输出查询、客户端 ID 或流地址。

源码安装需要 Rust stable 与本地链接器：Windows MSVC 构建工具/SDK，或 Linux C 编译器及 CMake。

```sh
git clone https://github.com/styayur/mzk.git
cd mzk
cargo build --release --locked --bin mzk
cargo install --path . --locked --bin mzk
```

## 测试与平台

```sh
cargo fmt --all -- --check
cargo check --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --release --locked --bin mzk
```

[CI](https://github.com/styayur/mzk/actions/workflows/ci.yml) 在 Windows 与 Linux 执行这些检查。`test-support` 只构建离线测试使用的模拟播放器，发行包只含 `mzk`。Mock 不代表真实网络或音频验证；已有 mpv 的可选集成测试见[验证记录](docs/VERIFICATION.md)。

## 限制、隐私与恢复

Jamendo 的独立音乐曲库不等于主流音乐曲库。字符匹配可能产生意外选择或无结果；小写映射并非完整 Unicode case folding。未知许可、缺失署名、非 HTTPS 或非 Jamendo 流地址会被保守拒绝。是否实际可播放由 mpv 状态确认。

切歌直接替换当前曲目，失败可能中断旧曲，不保证回滚、无缝切换。记录的进程仍存活而 IPC 无法验证时，会报错并要求检查该 PID，不启动另一个长期播放器。进程已退出或 PID 代际改变时，下次调用清理旧状态。损坏的 JSON 留给用户检查，不自动猜测归属。

仅保存 `player.json`（PID、创建身份、随机端点 token）及 `launch.lock`，Windows 位于 `%LOCALAPPDATA%\mzk`，Linux 位于 `$XDG_RUNTIME_DIR/mzk-UID`，未设置时使用系统临时目录；Linux 另有临时 socket。不保存搜索、曲目元数据或音频。mpv 仅做内存缓冲；查询和客户端 ID 直接发送给 Jamendo，mpv 直接访问音源服务器，提供方可看到网络元数据。

IPC 以可信的当前用户会话为边界。运行目录受权限保护；mpv >=0.40 的 Windows 管道限当前用户并拒绝远程访问；Linux 使用 0700 目录及 Unix socket。PID、创建身份、系统端点 PID 与 mpv 自报 PID 必须一致。管理员、恶意同用户进程及不可信 mpv 不在防护范围内。进程创建后、状态落盘前崩溃可能留下未登记播放器，须按 IPC token 精确识别。参见[安全与恢复说明](SECURITY.md)，不要按名称批量终止 mpv。

确认播放意味着 mpv 已解码音频、路径匹配且未暂停或空闲，不能证明扬声器发声。冷启动 P50 ≤2.5 秒、P95 ≤5 秒仅为测量目标，尚无真实播放分位数。

## 后续与贡献

下一步是双平台真实播放、终端关闭、全新安装验证，以及首次启动和切歌性能测量。请在 [Issues](https://github.com/styayur/mzk/issues) 提供 OS、mpv 版本和脱敏指标，提交前运行上述检查并保持单命令和隐私边界。安全问题见 [SECURITY.md](SECURITY.md)。

## 许可证

一方代码与文档为 [MIT](LICENSE)，版权归 styayur。音乐遵循逐曲许可及 Jamendo 条款，依赖和 mpv 保留原许可。仓库不分发音频或 mpv；详见[第三方边界](THIRD_PARTY.md)。
