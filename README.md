# UOG项目

UOG是一个基于Rust开发的UDP over gRPC工具。它允许通过gRPC隧道传输UDP流量,为UDP通信提供了额外的安全性和灵活性。

## 主要特性

- 使用gRPC作为传输层,提供加密和认证
- 支持UDP到gRPC的双向转换
- 跨平台支持,包括Linux、macOS、Windows和Android
- 高性能设计,适用于实时应用场景
- 简单易用的命令行界面

## 应用场景

UOG可以应用于以下场景:

- 在限制性网络环境中传输UDP流量
- 为游戏、VoIP等UDP应用提供额外的安全层
- 在企业网络中安全地传输UDP数据
- 作为VPN或代理服务的组件

## 技术栈

- Rust编程语言
- Tokio异步运行时
- Tonic gRPC框架
- Protocol Buffers用于数据序列化
- Cargo用于项目管理和构建

## 许可证

本项目采用Apache 2.0许可证。详情请参阅[LICENSE](LICENSE)文件。

## 构建

本项目使用 Cargo 进行构建管理。以下是在本地构建项目的步骤:

### 前置要求

- Rust 工具链 (推荐使用 rustup 安装)
- Protobuf 编译器 (protoc)
- 如果构建 Android 库,需要 Android NDK

### 构建步骤

1. 克隆仓库:

   ```bash
   git clone https://github.com/your-repo/uog.git
   cd uog
   ```

2. 安装必要的工具:

   ```bash
   cargo install cargo-ndk
   cargo install cross --git https://github.com/cross-rs/cross
   ```

3. 构建二进制文件:

   ```bash
   cargo build --release
   ```

   构建完成后,你可以在 `target/release/` 目录下找到 `uog` 可执行文件。

4. 构建 Android 库 (可选):

   确保你已经安装了 Android NDK 并设置了正确的环境变量,然后运行:

   ```bash
   cross build --target aarch64-linux-android --release --lib
   ```

   构建完成后,你可以在 `target/aarch64-linux-android/release/` 目录下找到 `libuog.so` 文件。

### 注意事项

- 确保你的 Rust 工具链是最新的。你可以使用 `rustup update` 来更新。
- 如果遇到构建问题,请检查 `Cargo.toml` 文件中的依赖版本是否与你的环境兼容。
- 对于跨平台构建,可能需要安装额外的工具链或依赖。请参考 Rust 官方文档以获取更多信息。
