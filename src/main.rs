use actix_web::{web, App, HttpResponse, HttpServer, Responder, Error};
use actix_multipart::Multipart;
use futures::{StreamExt, TryStreamExt};
use tempfile::tempdir;
use std::io::Write;
use std::fs;
use std::process::Command;
use std::path::Path;
use serde::Deserialize;

/// クエリパラメータ用の構造体
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

    // 一時ディレクトリを作成
    let temp_dir = tempdir().map_err(|_| {
        actix_web::error::ErrorInternalServerError("一時ディレクトリの作成に失敗しました")
    })?;
    let temp_path = temp_dir.path();

    // multipart/form-dataの解析：フィールド名が "files" のものを処理
    while let Some(item) = payload.next().await {
        let mut field = item?;
        if field.name() != "files" {
            continue;
        }
        // アップロードされたファイル名を取得（無い場合は "file" とする）
        let filename = field
            .content_disposition()
            .get_filename()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "file".to_owned());
        let filepath = temp_path.join(&filename);
        let mut f = fs::File::create(&filepath).map_err(|_| {
            actix_web::error::ErrorInternalServerError("ファイルの作成に失敗しました")
        })?;
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

    // zipコマンドを利用してパスワード付きZIPファイルを作成
    // -j: パス情報を除去、-P: パスワード指定
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

    // 作成されたZIPファイルを読み込み、レスポンスとして返す
    let zip_path = temp_path.join(&zip_filename);
    let zip_data = fs::read(&zip_path).map_err(|_| {
        actix_web::error::ErrorInternalServerError("ZIPファイルの読み込みに失敗しました")
    })?;

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

//
// 以下、テストコード
//
#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use futures::stream;
    use actix_multipart::Multipart;
    use bytes::Bytes;

    // テスト用に空のmultipartストリームを作成する関数
    fn empty_multipart() -> Multipart {
        // futures::stream::empty() は空のストリームを返す
        let empty_stream = stream::empty::<Result<actix_multipart::Field, actix_web::Error>>();
        Multipart::new(empty_stream)
    }

    // パスワードが指定されていない場合のテスト
    #[actix_rt::test]
    async fn test_create_zip_missing_password() {
        let mut app = test::init_service(
            App::new().route("/createzip", web::post().to(create_zip))
        ).await;

        let req = test::TestRequest::post()
            .uri("/createzip")
            .set_payload("dummy")
            .to_request();
        let resp = test::call_service(&mut app, req).await;
        assert_eq!(resp.status(), 400);
    }

    // ファイルがアップロードされていない場合のテスト
    #[actix_rt::test]
    async fn test_create_zip_no_files() {
        // クエリパラメータにパスワードを指定
        let query = web::Query(CreateZipParams {
            password: "secret".into(),
            zip_filename: None,
        });
        // 空のmultipartストリームを作成
        let multipart = empty_multipart();
        let resp = create_zip(multipart, query).await.unwrap();
        // アップロードファイルがないため、BadRequest (400)が返る
        assert_eq!(resp.status(), 400);
    }
}