cross build  --target aarch64-linux-android --release --lib
cargo run --features=uniffi/cli --bin uniffi generate --library target/aarch64-linux-android/release/libuog.so --language kotlin --out-dir out