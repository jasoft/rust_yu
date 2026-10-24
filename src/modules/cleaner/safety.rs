use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::scanner::models::{Trace, TraceType};
use crate::modules::scanner::scope::ScanIdentity;
use winreg::{RegKey, RegValue};

/// 关键系统路径黑名单
const CRITICAL_PATHS: &[&str] = &[
    r"C:\Windows",
    r"C:\Windows\System32",
    r"C:\Windows\SysWOW64",
    r"C:\Windows\WinSxS",
    r"C:\Windows\INF",
    r"C:\Windows\DriverStore",
    r"C:\Windows\System32\DriverStore",
];

/// 关键注册表路径黑名单
const CRITICAL_REGISTRY_PATHS: &[&str] = &[
    r"HKLM\SYSTEM",
    r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
    r"HKCR\*",
    r"HKLM\BOOT",
    r"HKLM\SAM",
    r"HKLM\SECURITY",
];

/// 删除前检查
pub fn pre_delete_check(trace: &Trace) -> Result<(), UninstallerError> {
    // 检查是否标记为关键项
    if trace.is_critical {
        return Err(UninstallerError::CriticalSystemItem(format!(
            "该项被标记为关键系统项: {}",
            trace.path
        )));
    }

    // 共用目录保护：仍被其他已安装程序使用的路径禁止删除，避免误伤其他程序
    if trace.is_shared {
        return Err(UninstallerError::CriticalSystemItem(format!(
            "该项仍被其他已安装程序共享使用，已跳过: {}",
            trace.path
        )));
    }

    // 根据类型进行特定检查
    match trace.trace_type {
        TraceType::RegistryKey | TraceType::RegistryValue => {
            if is_critical_registry(&trace.path) {
                return Err(UninstallerError::CriticalSystemItem(
                    "不能删除关键系统注册表项".to_string(),
                ));
            }
        }
        TraceType::File | TraceType::AppData | TraceType::Shortcut => {
            if is_critical_path(&trace.path) {
                return Err(UninstallerError::CriticalSystemItem(
                    "不能删除关键系统目录".to_string(),
                ));
            }
        }
        _ => {}
    }

    Ok(())
}

/// 检查是否为关键系统路径
fn is_critical_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    for critical in CRITICAL_PATHS {
        if path_upper.starts_with(&critical.to_uppercase()) {
            return true;
        }
    }

    false
}

/// 检查是否为关键注册表路径
fn is_critical_registry(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    for critical in CRITICAL_REGISTRY_PATHS {
        if path_upper.starts_with(critical) {
            return true;
        }
    }

    false
}

/// 列出所有关键路径（用于显示）
#[allow(dead_code)]
pub fn get_critical_paths() -> &'static [&'static str] {
    CRITICAL_PATHS
}

/// 列出所有关键注册表路径（用于显示）
#[allow(dead_code)]
pub fn get_critical_registry_paths() -> &'static [&'static str] {
    CRITICAL_REGISTRY_PATHS
}

/// 共享"应用值容器"分支：键本体属于系统共享状态（绝不能删键），但其中
/// 单个值可能属于具体应用（Run 启动项、App Paths、应用环境变量、防火墙
/// 规则等）。这类值允许**值级删除**（键保留），与整段共享区拦截互补：
/// 拦截负责"不要删共享键/目录"，白名单负责"应用私有的值可以清理"。
/// 匹配必须以"注册表键组件序列的精确后缀"判定，禁止无边界子串包含，
/// 以免意外命中路径中段恰好相似的分支。
const SHARED_APPLICATION_VALUE_CONTAINERS: &[&[&str]] = &[
    &["microsoft", "windows", "currentversion", "run"],
    &["microsoft", "windows", "currentversion", "runonce"],
    &["microsoft", "windows", "currentversion", "app paths"],
    &["currentcontrolset", "control", "session manager", "environment"],
    &[
        "services",
        "sharedaccess",
        "parameters",
        "firewallpolicy",
        "firewallrules",
    ],
];

/// Session Manager\Environment 里的值多为系统全局配置（Path/TEMP/...），
/// 值数据天然引用众多软件目录，绝不能拿"数据引用"当归属证据；
/// 该容器只接受"值名即身份"这一种证据（如 RUSTYU_DIRTY_TRACES_HOME）。
const NAME_ONLY_CONTAINERS: &[&[&str]] = &[&[
    "currentcontrolset",
    "control",
    "session manager",
    "environment",
]];

/// 解析注册表值 trace 的 (hive, 键路径组件(小写), 真实值名)。
/// 尾组件 `(Default)` 是展示名，真实值名为空串。
fn parse_value_trace(
    trace: &Trace,
) -> Option<(winreg::HKEY, Vec<String>, String)> {
    let (hkey, subkey) = utils::parse_registry_path(&trace.path)?;
    // rsplit_once 返回 (其前的键路径, 最后一个组件=值名)
    let (key_path, value_display_name) = subkey.rsplit_once('\\')?;
    let value_name = match value_display_name {
        "(Default)" | "(default)" => String::new(),
        other => other.to_string(),
    };
    let components = key_path
        .split('\\')
        .map(|component| component.trim().to_lowercase())
        .filter(|component| !component.is_empty())
        .collect();
    Some((hkey, components, value_name))
}

fn components_end_with(components: &[String], suffix: &[&str]) -> bool {
    if components.len() < suffix.len() {
        return false;
    }
    components[components.len() - suffix.len()..]
        .iter()
        .zip(suffix.iter())
        .all(|(component, expected)| component == expected)
}

/// 容器键匹配：键路径以容器结尾（值直接位于容器键，如 Run\DemoApp），或
/// 以"容器 + 恰好一个应用命名子键"结尾（如 App Paths\demo.exe 下的值）。
/// 第二形态只用于 App Paths 这类"每应用一个子键"的分支；值级删除仍必须
/// 通过 revalidate_shared_value_ownership 的归属证据兜底。
fn key_components_match_container(components: &[String], suffix: &[&str]) -> bool {
    components_end_with(components, suffix)
        || (components.len() >= suffix.len() + 1
            && components_end_with(&components[..components.len() - 1], suffix))
}

/// 判断 trace 是否是共享应用值容器里的 RegistryValue（可走值级删除通道）。
pub fn is_shared_application_value_container(trace: &Trace) -> bool {
    if trace.trace_type != TraceType::RegistryValue {
        return false;
    }
    let Some((_hkey, components, _value_name)) = parse_value_trace(trace) else {
        return false;
    };
    SHARED_APPLICATION_VALUE_CONTAINERS
        .iter()
        .any(|suffix| key_components_match_container(&components, suffix))
}

/// 值级删除前的归属重校验：键保留、只删一个值，但这个值必须仍然与程序
/// 身份强关联，防止共享键中他人值（尤其 Path 这类全局配置）被误删。
///
/// 证据规则：
/// 1. 值名即身份——值名 compact 后与身份别名精确相等（Run\DemoApp、
///    Environment\RUSTYU_DIRTY_TRACES_HOME）；
/// 2. 值数据以完整路径/词组件引用程序——程序显示名或 compact 别名作为
///    整个组件出现（两侧为路径分隔符/引号/字段边界），裸子串命中不算。
///    Environment 容器禁用此证据（见 NAME_ONLY_CONTAINERS）。
pub fn revalidate_shared_value_ownership(trace: &Trace) -> Result<(), UninstallerError> {
    let Some((hkey, components, value_name)) = parse_value_trace(trace) else {
        return Err(UninstallerError::CriticalSystemItem(format!(
            "无法解析注册表值路径: {}",
            trace.path
        )));
    };
    let key_path = components.join("\\");

    let key = match RegKey::predef(hkey).open_subkey(&key_path) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // 键已随其他清理（例如整键删除）消失：值级删除按幂等成功处理
            return Ok(());
        }
        Err(error) => {
            return Err(UninstallerError::Registry(format!(
                "打开共享容器键失败: {error}"
            )));
        }
    };
    let raw: RegValue = match key.get_raw_value(&value_name) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // 值已消失，幂等成功
            return Ok(());
        }
        Err(error) => {
            return Err(UninstallerError::Registry(format!(
                "读取共享容器值失败: {error}"
            )));
        }
    };

    let identity = ScanIdentity::from_name(&trace.program_name);
    let value_name_compact =
        crate::modules::scanner::scope::compact_identifier(&value_name);

    // 证据 1：值名即身份（对任何容器都有效，也是 Environment 的唯一证据）
    let name_is_identity = identity
        .aliases()
        .iter()
        .any(|alias| !alias.is_empty() && alias == &value_name_compact);

    // 证据 2：值数据以完整组件引用程序显示名/别名
    let data_allowed = !NAME_ONLY_CONTAINERS
        .iter()
        .any(|suffix| components_end_with(&components, suffix));
    let data_text = decode_registry_value_text(&raw);
    let data_references_identity = data_allowed
        && data_references_identity_as_component(&data_text, &identity);

    if name_is_identity || data_references_identity {
        Ok(())
    } else {
        Err(UninstallerError::CriticalSystemItem(format!(
            "共享键中的值不再归属于该程序，已跳过: {}",
            trace.path
        )))
    }
}

/// 程序显示名或 compact 别名以"完整组件"形式出现在值数据中：
/// 命中位置两侧必须是路径分隔符、引号、字段分隔符或字符串边界。
fn data_references_identity_as_component(text: &str, identity: &ScanIdentity) -> bool {
    if text.is_empty() {
        return false;
    }
    let haystack = text.to_lowercase();
    let display_lower = identity.display_name().trim().to_lowercase();
    let mut needles: Vec<&str> = Vec::new();
    if display_lower.len() >= 4 {
        needles.push(display_lower.as_str());
    }
    needles.extend(identity.aliases().iter().map(String::as_str));
    needles
        .iter()
        .any(|needle| !needle.is_empty() && contains_as_delimited_component(&haystack, needle))
}

fn contains_as_delimited_component(haystack: &str, needle: &str) -> bool {
    let is_delimiter = |byte: u8| {
        matches!(
            byte,
            b'\\' | b'/' | b'"' | b'\'' | b';' | b'|' | b'=' | b',' | b' ' | b'\t' | b'\r' | b'\n'
        )
    };
    let haystack_bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut start = 0usize;
    while let Some(found) = haystack[start..].find(needle) {
        let absolute = start + found;
        let before_ok = absolute == 0 || is_delimiter(haystack_bytes[absolute - 1]);
        let after = absolute + needle_bytes.len();
        let after_ok = after >= haystack_bytes.len() || is_delimiter(haystack_bytes[after]);
        if before_ok && after_ok {
            return true;
        }
        start = absolute + 1;
    }
    false
}

/// 按注册表的真实编码把值转为小写文本用于归属比对：
/// REG_SZ/REG_EXPAND_SZ/MULTI_SZ 的字节是 UTF-16LE（绝不能按 UTF-8 解码），
/// 数值转十进制，REG_BINARY 不产生文本证据（返回空串）。
fn decode_registry_value_text(value: &RegValue) -> String {
    match value.vtype {
        winreg::enums::REG_SZ
        | winreg::enums::REG_EXPAND_SZ
        | winreg::enums::REG_MULTI_SZ => {
            let units: Vec<u16> = value
                .bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect();
            String::from_utf16_lossy(&units).to_lowercase()
        }
        winreg::enums::REG_DWORD => {
            let number = if value.bytes.len() == 4 {
                u32::from_le_bytes([value.bytes[0], value.bytes[1], value.bytes[2], value.bytes[3]])
            } else {
                0
            };
            number.to_string()
        }
        winreg::enums::REG_QWORD => {
            let number = if value.bytes.len() == 8 {
                u64::from_le_bytes([
                    value.bytes[0],
                    value.bytes[1],
                    value.bytes[2],
                    value.bytes[3],
                    value.bytes[4],
                    value.bytes[5],
                    value.bytes[6],
                    value.bytes[7],
                ])
            } else {
                0
            };
            number.to_string()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_shared_application_value_container, pre_delete_check,
        revalidate_shared_value_ownership,
    };
    use crate::modules::scanner::models::{Trace, TraceType};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    fn trace(trace_type: TraceType, path: &str) -> Trace {
        Trace::new("Demo App".to_string(), trace_type, path.to_string())
    }

    #[test]
    fn pre_delete_check_rejects_items_marked_as_critical() {
        let mut trace = trace(TraceType::File, r"C:\Users\soj\AppData\Local\Demo App");
        trace.is_critical = true;

        assert!(pre_delete_check(&trace).is_err());
    }

    #[test]
    fn pre_delete_check_rejects_critical_system_directories() {
        let trace = trace(TraceType::File, r"C:\Windows\System32\demo.dll");

        assert!(pre_delete_check(&trace).is_err());
    }

    #[test]
    fn pre_delete_check_rejects_critical_registry_paths() {
        let trace = trace(
            TraceType::RegistryKey,
            r"HKLM\SYSTEM\CurrentControlSet\Services\DemoApp",
        );

        assert!(pre_delete_check(&trace).is_err());
    }

    #[test]
    fn pre_delete_check_allows_non_critical_paths() {
        let trace = trace(
            TraceType::File,
            r"C:\Users\soj\AppData\Local\Demo App\cache",
        );

        assert!(pre_delete_check(&trace).is_ok());
    }

    #[test]
    fn shared_value_container_detection_matches_only_value_traces_in_known_branches() {
        // 只有已知"应用值容器"分支里的 RegistryValue 才走值级删除通道；
        // 键 trace 与其他共享分支（如 Explorer）保持原有整段拦截。
        assert!(is_shared_application_value_container(&trace(
            TraceType::RegistryValue,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\DemoApp",
        )));
        assert!(is_shared_application_value_container(&trace(
            TraceType::RegistryValue,
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run\DemoApp",
        )));
        assert!(is_shared_application_value_container(&trace(
            TraceType::RegistryValue,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\demo.exe\(Default)",
        )));
        assert!(!is_shared_application_value_container(&trace(
            TraceType::RegistryKey,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\DemoApp",
        )));
        assert!(!is_shared_application_value_container(&trace(
            TraceType::RegistryValue,
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\DemoValue",
        )));
        // 无边界子串不应误命中：路径中段出现 run 字样不算容器
        assert!(!is_shared_application_value_container(&trace(
            TraceType::RegistryValue,
            r"HKCU\Software\SomeRunVendor\CurrentVersion\Demo",
        )));
    }

    #[test]
    fn shared_value_revalidation_accepts_owned_value_and_rejects_strangers() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let base = format!(r"Software\rust-yu-test\{suffix}\SafetyCheck");
        let hive = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hive.create_subkey(&base).expect("应能创建测试键");
        key.set_value("DemoApp", &r#""C:\Program Files\Demo App\demo.exe""#)
            .expect("应能写入自有值");
        key.set_value("DataRef", &r#""C:\Program Files\Demo App\tool.exe""#)
            .expect("应能写入数据引用值");
        key.set_value("Unrelated", &"totally different program")
            .expect("应能写入他人值");
        key.set_value("Substring", &r#""C:\Tools\MyDemoAppHelper\x.exe""#)
            .expect("应能写入子串干扰值");

        // 值名即身份：允许
        let owned = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\DemoApp"),
        );
        assert!(revalidate_shared_value_ownership(&owned).is_ok());
        // 值数据以完整组件引用安装目录（UTF-16 SZ 值）：允许
        let data_ref = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\DataRef"),
        );
        assert!(revalidate_shared_value_ownership(&data_ref).is_ok());
        // 他人值：必须拒绝
        let stranger = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\Unrelated"),
        );
        assert!(revalidate_shared_value_ownership(&stranger).is_err());
        // 程序名只作为更长组件的子串出现（MyDemoAppHelper）：不得当作归属证据
        let substring = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\Substring"),
        );
        assert!(revalidate_shared_value_ownership(&substring).is_err());
        // 值不存在：幂等成功
        let missing = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\Gone"),
        );
        assert!(revalidate_shared_value_ownership(&missing).is_ok());

        let _ = hive.delete_subkey_all(&base);
    }

    #[test]
    fn environment_container_rejects_data_reference_and_allows_name_identity_only() {
        // Session Manager\Environment 是系统全局配置（Path/TEMP 等），
        // 值数据天然引用众多软件目录：只接受"值名即身份"，数据证据必须失效
        let suffix = uuid::Uuid::new_v4().to_string();
        let base = format!(
            r"Software\rust-yu-test\{suffix}\CurrentControlSet\Control\Session Manager\Environment"
        );
        let hive = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hive.create_subkey(&base).expect("应能创建测试环境键");
        key.set_value("Path", &r"C:\Windows;C:\Program Files\Demo App\bin;C:\Tools")
            .expect("应能写入系统 Path 值");
        key.set_value("DEMO_APP_HOME", &r"C:\Program Files\Demo App")
            .expect("应能写入应用专属变量");

        let path_trace = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\Path"),
        );
        assert!(
            revalidate_shared_value_ownership(&path_trace).is_err(),
            "系统 Path 即使数据里出现程序目录也绝不允许值级删除"
        );
        // 现状：环境变量名通常是"程序名+用途后缀"（RUSTYU_DIRTY_TRACES_HOME），
        // 不等于身份别名。按安全优先原则一律拒绝并列为未完成项；待实现
        // "别名+白名单用途后缀"的精确规则后再放行（见验收文档待办）。
        let suffixed = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\DEMO_APP_HOME"),
        );
        assert!(
            revalidate_shared_value_ownership(&suffixed).is_err(),
            "带用途后缀的环境变量名在精确规则落地前必须拒绝"
        );

        let _ = hive.delete_subkey_all(&base);
    }

    #[test]
    fn default_value_trace_maps_to_empty_value_name() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let base = format!(r"Software\rust-yu-test\{suffix}\App Paths\demo.exe");
        let hive = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hive.create_subkey(&base).expect("应能创建测试键");
        // 写默认值（空名）
        key.set_value("", &r"C:\Program Files\Demo App\demo.exe")
            .expect("应能写入默认值");

        let default_trace = trace(
            TraceType::RegistryValue,
            &format!(r"HKCU\{base}\(Default)"),
        );
        assert!(revalidate_shared_value_ownership(&default_trace).is_ok());

        let _ = hive.delete_subkey_all(&base);
    }
}
