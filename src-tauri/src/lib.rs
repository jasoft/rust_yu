pub mod commands;

use commands::*;

use tauri::Manager;
use warp::Filter;

// 启动 HTTP API 服务器（用于开发模式）
fn start_api_server() {
    use rust_yu_lib::modules::lister;

    // 获取程序列表的 API 路由
    let programs_route = warp::path!("api" / "programs")
        .and(warp::get())
        .map(move || {
            // 调用主项目的 list_all_programs 函数
            let result = lister::list_all_programs(None, None);

            // 转换为 API 响应格式
            let response: Vec<serde_json::Value> = match result {
                Ok(programs) => programs
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
                            "estimated_size": p.estimated_size,
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

    let routes = programs_route;

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
