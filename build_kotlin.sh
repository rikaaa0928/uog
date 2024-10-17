cross build  --target aarch64-linux-android --release --lib
mkdir -p out/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libuog.so out/jniLibs/arm64-v8a/libuniffi_uog.so
cargo run --features=uniffi/cli --bin uniffi generate --library target/aarch64-linux-android/release/libuog.so --language kotlin --out-dir out