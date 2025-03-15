# ビルドステージ
FROM rust:1.70 as builder
WORKDIR /usr/src/app

# 静的リンク用にmuslターゲットを追加
RUN rustup target add x86_64-unknown-linux-musl

# 依存関係キャッシュ用にCargo.tomlとCargo.lockを先にコピー
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl

# ソースコード全体をコピーし、本番用バイナリをビルド
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

# 実行ステージ：distrolessのstaticイメージを使用
FROM gcr.io/distroless/static:nonroot
WORKDIR /app
# Cargoプロジェクト名（例: zip_webapp）に合わせてバイナリ名を指定
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/zip_webapp .
EXPOSE 8080
USER nonroot:nonroot
CMD ["/app/zip_webapp"]