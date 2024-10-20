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
