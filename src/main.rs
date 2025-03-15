use actix_web::{web, App, HttpResponse, HttpServer, Responder, Error};
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use tempfile::tempdir;
use std::io::Write;
use std::fs;
use std::process::Command;
use serde::Deserialize;

/// クエリパラメータの構造体（passwordは必須、zip_filenameは空の場合"protected.zip"とする）
#[derive(Deserialize)]
struct CreateZipParams {
    password: String,
    zip_filename: Option<String>,
}

/// /createzip エンドポイント
async fn create_zip(
    mut payload: Multipart,
    query: web::Query<CreateZipParams>,
) -> Result<HttpResponse, Error> {
    // パスワードチェック
    let password = query.password.trim();
    if password.is_empty() {
        return Ok(HttpResponse::BadRequest().body("Password is required"));
    }
    let zip_filename = query
        .zip_filename
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("protected.zip")
        .to_string();

    // 一時ディレクトリの作成（処理終了時に自動削除）
    let temp_dir = tempdir().map_err(|_| {
        actix_web::error::ErrorInternalServerError("一時ディレクトリの作成に失敗しました")
    })?;
    let temp_path = temp_dir.path();

    // multipart/form-dataの解析
    // フィールド名が "files" のもののみを処理する
    while let Some(item) = payload.next().await {
        let mut field = item?;
        if field.name() != "files" {
            continue;
        }
        // アップロードファイル名を取得。無い場合は "file" とする
        let filename = field
            .content_disposition()
            .get_filename()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "file".to_owned());
        let filepath = temp_path.join(&filename);
        let mut f = fs::File::create(&filepath).map_err(|_| {
            actix_web::error::ErrorInternalServerError("ファイルの作成に失敗しました")
        })?;
        // ファイル内容を一時ファイルへ書き出す
        while let Some(chunk) = field.next().await {
            let data = chunk.map_err(|_| {
                actix_web::error::ErrorInternalServerError("ファイルの読み込みに失敗しました")
            })?;
            f.write_all(&data).map_err(|_| {
                actix_web::error::ErrorInternalServerError("ファイルへの書き込みに失敗しました")
            })?;
        }
    }

    // 一時ディレクトリ内のアップロードファイル一覧を取得
    let mut file_list: Vec<String> = Vec::new();
    for entry in fs::read_dir(temp_path).map_err(|_| {
        actix_web::error::ErrorInternalServerError("一時ディレクトリの読み込みに失敗しました")
    })? {
        let entry = entry.map_err(|_| {
            actix_web::error::ErrorInternalServerError("ディレクトリエントリの読み込みに失敗しました")
        })?;
        let path = entry.path();
        if path.is_file() {
            if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                file_list.push(fname.to_string());
            }
        }
    }

    if file_list.is_empty() {
        return Ok(HttpResponse::BadRequest().body("ファイルがアップロードされていません"));
    }

    // zipコマンドを呼び出して暗号化ZIPファイルを作成
    // -j オプションでパス情報を除去、-P でパスワード指定
    let mut command = Command::new("zip");
    command.current_dir(temp_path);
    command.arg("-j")
           .arg("-P")
           .arg(password)
           .arg(&zip_filename)
           .args(&file_list);

    let output = command.output().map_err(|_| {
        actix_web::error::ErrorInternalServerError("zipコマンドの実行に失敗しました")
    })?;
    if !output.status.success() {
        return Ok(HttpResponse::InternalServerError().body("ZIPファイルの作成に失敗しました"));
    }

    // 作成したZIPファイルを読み込む
    let zip_path = temp_path.join(&zip_filename);
    let zip_data = fs::read(&zip_path).map_err(|_| {
        actix_web::error::ErrorInternalServerError("ZIPファイルの読み込みに失敗しました")
    })?;

    // ZIPファイルをレスポンスとして返す
    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .append_header(("Content-Disposition", format!("attachment; filename={}", zip_filename)))
        .body(zip_data))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("サーバーをポート8080で起動します...");
    HttpServer::new(|| {
        App::new()
            .route("/createzip", web::post().to(create_zip))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}