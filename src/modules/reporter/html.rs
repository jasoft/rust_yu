use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::cleaner::models::CleanResult;
use super::models::UninstallerReport;

/// 生成 HTML 报告
pub fn generate_html_report(report: &UninstallerReport) -> Result<String, UninstallerError> {
    let html = format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>卸载报告 - {}</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            padding: 20px;
        }}
        .container {{
            max-width: 900px;
            margin: 0 auto;
            background: white;
            border-radius: 16px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.3);
            overflow: hidden;
        }}
        .header {{
            background: linear-gradient(135deg, #2c3e50 0%, #34495e 100%);
            color: white;
            padding: 30px;
        }}
        .header h1 {{
            font-size: 28px;
            margin-bottom: 10px;
        }}
        .header .meta {{
            opacity: 0.8;
            font-size: 14px;
        }}
        .summary {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            padding: 30px;
            background: #f8f9fa;
        }}
        .stat {{
            background: white;
            padding: 20px;
            border-radius: 12px;
            text-align: center;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        }}
        .stat .value {{
            font-size: 32px;
            font-weight: bold;
            color: #667eea;
        }}
        .stat .label {{
            color: #666;
            margin-top: 8px;
            font-size: 14px;
        }}
        .success .value {{ color: #27ae60; }}
        .failed .value {{ color: #e74c3c; }}
        .content {{
            padding: 30px;
        }}
        .section-title {{
            font-size: 18px;
            color: #2c3e50;
            margin-bottom: 20px;
            padding-bottom: 10px;
            border-bottom: 2px solid #667eea;
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin-bottom: 20px;
        }}
        th, td {{
            padding: 12px 15px;
            text-align: left;
            border-bottom: 1px solid #eee;
        }}
        th {{
            background: #f8f9fa;
            color: #2c3e50;
            font-weight: 600;
        }}
        tr:hover {{
            background: #f8f9fa;
        }}
        .status {{
            display: inline-block;
            padding: 4px 12px;
            border-radius: 20px;
            font-size: 12px;
            font-weight: 600;
        }}
        .status.success {{
            background: #d4edda;
            color: #155724;
        }}
        .status.failed {{
            background: #f8d7da;
            color: #721c24;
        }}
        .type-badge {{
            display: inline-block;
            padding: 4px 10px;
            border-radius: 6px;
            font-size: 12px;
            background: #e9ecef;
            color: #495057;
        }}
        .path {{
            font-family: "Consolas", monospace;
            font-size: 13px;
            color: #666;
            word-break: break-all;
        }}
        .warnings {{
            background: #fff3cd;
            border-left: 4px solid #ffc107;
            padding: 15px 20px;
            margin-bottom: 20px;
        }}
        .warnings h3 {{
            color: #856404;
            margin-bottom: 10px;
        }}
        .warnings ul {{
            margin-left: 20px;
            color: #856404;
        }}
        .footer {{
            background: #f8f9fa;
            padding: 20px 30px;
            text-align: center;
            color: #666;
            font-size: 13px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>卸载报告</h1>
            <div class="meta">
                <p>程序: <strong>{}</strong></p>
                <p>生成时间: {}</p>
                <p>报告ID: {}</p>
            </div>
        </div>

        <div class="summary">
            <div class="stat">
                <div class="value">{}</div>
                <div class="label">发现痕迹</div>
            </div>
            <div class="stat success">
                <div class="value">{}</div>
                <div class="label">成功删除</div>
            </div>
            <div class="stat failed">
                <div class="value">{}</div>
                <div class="label">删除失败</div>
            </div>
            <div class="stat">
                <div class="value">{}</div>
                <div class="label">释放空间</div>
            </div>
        </div>

        <div class="content">
            {}
        </div>

        <div class="footer">
            <p>由 Awake-Windows 卸载工具生成</p>
        </div>
    </div>
</body>
</html>"#,
        report.program_name,
        report.program_name,
        report.generated_at.format("%Y-%m-%d %H:%M:%S"),
        report.id,
        report.traces_found.len(),
        report.traces_removed.iter().filter(|r| r.success).count(),
        report.traces_removed.iter().filter(|r| !r.success).count(),
        utils::format_size(report.total_size_freed),
        generate_results_table(&report.traces_removed),
    );

    Ok(html)
}

fn generate_results_table(results: &[CleanResult]) -> String {
    if results.is_empty() {
        return "<p>暂无删除记录</p>".to_string();
    }

    let mut html = String::from(r#"
        <h2 class="section-title">删除详情</h2>
        <table>
            <thead>
                <tr>
                    <th>状态</th>
                    <th>类型</th>
                    <th>路径</th>
                    <th>释放空间</th>
                </tr>
            </thead>
            <tbody>
    "#);

    for result in results {
        let status_html = if result.success {
            r#"<span class="status success">成功</span>"#
        } else {
            r#"<span class="status failed">失败</span>"#
        };

        let size_html = if result.bytes_freed > 0 {
            utils::format_size(result.bytes_freed)
        } else {
            "-".to_string()
        };

        // 尝试从路径推断类型
        let type_html = if result.path.contains("HKLM") || result.path.contains("HKCU") || result.path.contains("HKCR") {
            r#"<span class="type-badge">注册表</span>"#
        } else if result.path.ends_with(".lnk") {
            r#"<span class="type-badge">快捷方式</span>"#
        } else if result.path.contains("AppData") {
            r#"<span class="type-badge">AppData</span>"#
        } else {
            r#"<span class="type-badge">文件/目录</span>"#
        };

        html.push_str(&format!(r#"
                <tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td class="path">{}</td>
                    <td>{}</td>
                </tr>
        "#,
            status_html,
            type_html,
            escape_html(&result.path),
            size_html,
        ));
    }

    html.push_str("</tbody></table>");

    html
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
