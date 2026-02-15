pub mod commands;

use commands::*;

use std::path::PathBuf;

use tauri::Manager;
use warp::Filter;

#[derive(Debug, serde::Deserialize)]
struct IconFileQuery {
    path: String,
}

// 启动 HTTP API 服务器（用于开发模式）
fn start_api_server() {
    use rust_yu_lib::modules::lister;

    // 获取程序列表的 API 路由
    let programs_route = warp::path!("api" / "programs")
        .and(warp::get())
        .map(move || {
            // 调用缓存版本接口，避免每次请求都重复做图标/大小计算
            let query = lister::models::ListProgramsQuery {
                source: Some(lister::models::InstallSource::Registry),
                search: None,
                refresh: false,
                cache_ttl_seconds: lister::storage::DEFAULT_CACHE_TTL_SECONDS,
            };
            let result = lister::list_programs_with_cache(query);

            // 转换为 API 响应格式
            let response: Vec<serde_json::Value> = match result {
                Ok(program_response) => program_response
                    .programs
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "id": p.id,
                            "name": p.name,
                            "publisher": p.publisher,
                            "version": p.version,
                            "install_location": p.install_location,
                            "install_date": p.install_date,
                            "uninstall_string": p.uninstall_string,
                            "install_source": p.install_source.to_string(),
                            "size": p.size,
                            "icon_path": p.icon_path,
                            "icon_cache_path_32": p.icon_cache_path_32,
                            "icon_cache_path_48": p.icon_cache_path_48,
                            "size_last_updated_at": p.size_last_updated_at,
                            "icon_data_url": p.icon_data_url,
                            "icon_data_url_32": p.icon_data_url_32,
                            "icon_data_url_48": p.icon_data_url_48,
                            "estimated_size": p.estimated_size,
                            "install_date_source": p.install_date_source,
                            "install_date_confidence": p.install_date_confidence,
                            "icon_source": p.icon_source,
                            "icon_confidence": p.icon_confidence,
                            "size_source": p.size_source,
                            "size_confidence": p.size_confidence,
                            "metadata_confidence": p.metadata_confidence,
                        })
                    })
                    .collect(),
                Err(e) => {
                    tracing::error!("Failed to list programs: {}", e);
                    vec![serde_json::json!({ "error": e.to_string() })]
                }
            };

            warp::reply::json(&response)
        });

    // 读取图标缓存文件（仅允许 icon-cache 目录）
    let icon_route = warp::path!("api" / "icon")
        .and(warp::get())
        .and(warp::query::<IconFileQuery>())
        .and_then(|query: IconFileQuery| async move {
            let icon_root = match lister::storage::get_icon_cache_dir() {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!("icon route: get icon cache dir failed: {}", error);
                    return Err(warp::reject::not_found());
                }
            };

            let requested_path = PathBuf::from(query.path);
            let canonical_root = match std::fs::canonicalize(&icon_root) {
                Ok(path) => path,
                Err(_) => return Err(warp::reject::not_found()),
            };
            let canonical_requested = match std::fs::canonicalize(&requested_path) {
                Ok(path) => path,
                Err(_) => return Err(warp::reject::not_found()),
            };

            if !canonical_requested.starts_with(&canonical_root) {
                tracing::warn!(
                    "icon route: rejected path outside icon cache: {}",
                    canonical_requested.to_string_lossy()
                );
                return Err(warp::reject::not_found());
            }

            match tokio::fs::read(&canonical_requested).await {
                Ok(bytes) => {
                    let response = warp::http::Response::builder()
                        .header("content-type", "image/png")
                        .body(bytes);
                    match response {
                        Ok(ok) => Ok(ok),
                        Err(_) => Err(warp::reject::not_found()),
                    }
                }
                Err(_) => Err(warp::reject::not_found()),
            }
        });

    // 仅开发调试使用：允许本地前端页面跨域读取程序列表
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET"])
        .allow_headers(vec!["content-type"]);

    let routes = programs_route.or(icon_route).with(cors);

    // 在后台线程启动服务器
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            warp::serve(routes).run(([127, 0, 0, 1], 8080)).await;
        });
    });

    tracing::info!("HTTP API 服务器已启动: http://localhost:8080");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // 启动 HTTP API 服务器（用于开发模式）
    // 也可以通过环境变量 RUST_YU_API_PORT 来指定端口
    if std::env::var("RUST_YU_ENABLE_API").is_ok() {
        start_api_server();
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            tracing::info!("Rust Yu Tauri 应用启动");
            let _ = app.get_webview_window("main");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_programs,
            search_programs,
            scan_traces,
            clean_traces,
            uninstall_program,
            get_reports,
            delete_report,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用时出错");
}
