# ビルドステージ
FROM rust:1.70 as builder
WORKDIR /usr/src/app

# 依存関係キャッシュ用にCargo.tomlとCargo.lockを先にコピー
COPY Cargo.toml Cargo.lock ./
# 仮のsrc/main.rsを配置して依存関係を先にビルド（キャッシュ利用のため）
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# 実際のソースコードをコピーし、リリースビルド
COPY . .
RUN cargo build --release

# 実行ステージ（Debian slim）
FROM debian:buster-slim
WORKDIR /app
# Cargoプロジェクト名（ここでは zip_webapp ）に合わせてバイナリをコピー
COPY --from=builder /usr/src/app/target/release/zip_webapp .
EXPOSE 8080
CMD ["./zip_webapp"]