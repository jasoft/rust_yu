use super::models::{CleanerEntry, Exclusion, ExclusionKind, FileKey, FileKeyFlag, RegistryKey};

pub(crate) fn parse_database(content: &str) -> Vec<CleanerEntry> {
    let mut entries = Vec::new();
    let mut current = None::<EntryBuilder>;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            push_if_valid(&mut entries, current.take());
            current = Some(EntryBuilder::new(name.trim_end_matches('*').trim()));
            continue;
        }
        let (Some(builder), Some((key, value))) = (current.as_mut(), line.split_once('=')) else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case("LangSecRef") {
            builder.lang_sec_ref = value.parse().ok();
        } else if key.eq_ignore_ascii_case("Section") {
            builder.section = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("Warning") {
            builder.warning = Some(value.to_string());
        } else if key.eq_ignore_ascii_case("Default") {
            builder.default_enabled = value.eq_ignore_ascii_case("true");
        } else if numbered_key(key, "Detect", true) {
            builder.detect_keys.push(value.to_string());
        } else if numbered_key(key, "DetectFile", true) {
            builder.detect_files.push(value.to_string());
        } else if numbered_key(key, "FileKey", false) {
            builder.file_keys.push(parse_file_key(value));
        } else if numbered_key(key, "RegKey", false) {
            builder.registry_keys.push(parse_registry_key(value));
        } else if numbered_key(key, "ExcludeKey", false) {
            builder.exclusions.push(parse_exclusion(value));
        }
    }
    push_if_valid(&mut entries, current);
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.id = format!("rule-{index}");
    }
    entries
}

pub(crate) fn database_version(content: &str) -> String {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("; Version:"))
        .map(str::trim)
        .unwrap_or("unknown")
        .to_string()
}

fn numbered_key(key: &str, prefix: &str, number_optional: bool) -> bool {
    let Some(suffix) = key.get(prefix.len()..) else {
        return false;
    };
    key.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        && ((number_optional && suffix.is_empty())
            || (!suffix.is_empty() && suffix.chars().all(|value| value.is_ascii_digit())))
}

fn parse_file_key(value: &str) -> FileKey {
    let parts: Vec<_> = value.split('|').map(str::trim).collect();
    let mut patterns = vec!["*".to_string()];
    let mut flag = FileKeyFlag::None;
    if let Some(second) = parts.get(1).filter(|part| !part.is_empty()) {
        match second.to_ascii_uppercase().as_str() {
            "RECURSE" => flag = FileKeyFlag::Recurse,
            "REMOVESELF" => flag = FileKeyFlag::RemoveSelf,
            _ => {
                patterns = second
                    .split(';')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(|item| if item == "*.*" { "*" } else { item })
                    .map(str::to_string)
                    .collect()
            }
        }
    }
    if let Some(third) = parts.get(2) {
        flag = match third.to_ascii_uppercase().as_str() {
            "RECURSE" => FileKeyFlag::Recurse,
            "REMOVESELF" => FileKeyFlag::RemoveSelf,
            _ => FileKeyFlag::None,
        };
    }
    FileKey {
        path: parts.first().copied().unwrap_or_default().to_string(),
        patterns,
        flag,
    }
}

fn parse_registry_key(value: &str) -> RegistryKey {
    let (path, value_name) = value.split_once('|').map_or((value, None), |(path, name)| {
        (path, Some(name.trim().to_string()))
    });
    RegistryKey {
        path: path.trim().to_string(),
        value_name,
    }
}

fn parse_exclusion(value: &str) -> Exclusion {
    let parts: Vec<_> = value.split('|').map(str::trim).collect();
    let kind = match parts
        .first()
        .copied()
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "PATH" => ExclusionKind::Path,
        "REG" => ExclusionKind::Registry,
        _ => ExclusionKind::File,
    };
    Exclusion {
        kind,
        path: parts.get(1).copied().unwrap_or_default().to_string(),
        pattern: parts
            .get(2)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string),
    }
}

fn category_name(code: Option<i32>, section: Option<&str>) -> String {
    match code {
        Some(3006) => "Microsoft Edge",
        Some(3021) => "应用程序",
        Some(3022) => "互联网",
        Some(3023) => "多媒体",
        Some(3024) => "实用工具",
        Some(3025) => "Windows",
        Some(3026) => "Firefox",
        Some(3027) => "Opera",
        Some(3029) => "Google Chrome",
        Some(3030) => "Thunderbird",
        Some(3031) => "Microsoft Store",
        Some(3033) => "Vivaldi",
        Some(3034) => "Brave",
        Some(3035) => "Opera GX",
        Some(3036) => "Spotify",
        _ => section.unwrap_or("其他应用"),
    }
    .to_string()
}

fn push_if_valid(entries: &mut Vec<CleanerEntry>, builder: Option<EntryBuilder>) {
    let Some(builder) = builder else { return };
    if (builder.detect_keys.is_empty() && builder.detect_files.is_empty())
        || (builder.file_keys.is_empty() && builder.registry_keys.is_empty())
    {
        return;
    }
    entries.push(builder.finish());
}

struct EntryBuilder {
    name: String,
    lang_sec_ref: Option<i32>,
    section: Option<String>,
    warning: Option<String>,
    default_enabled: bool,
    detect_keys: Vec<String>,
    detect_files: Vec<String>,
    file_keys: Vec<FileKey>,
    registry_keys: Vec<RegistryKey>,
    exclusions: Vec<Exclusion>,
}

impl EntryBuilder {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            lang_sec_ref: None,
            section: None,
            warning: None,
            default_enabled: true,
            detect_keys: Vec::new(),
            detect_files: Vec::new(),
            file_keys: Vec::new(),
            registry_keys: Vec::new(),
            exclusions: Vec::new(),
        }
    }
    fn finish(self) -> CleanerEntry {
        CleanerEntry {
            id: String::new(),
            name: self.name,
            category: category_name(self.lang_sec_ref, self.section.as_deref()),
            warning: self.warning,
            default_enabled: self.default_enabled,
            detect_keys: self.detect_keys,
            detect_files: self.detect_files,
            file_keys: self.file_keys,
            registry_keys: self.registry_keys,
            exclusions: self.exclusions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{database_version, parse_database};

    #[test]
    fn parses_winapp2_rule() {
        let rules = parse_database(
            "; Version: 1\n[Demo *]\nDetectFile=%Temp%\\Demo\nFileKey1=%Temp%\\Demo|*.*|RECURSE\n",
        );
        assert_eq!(database_version("; Version: 1"), "1");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].file_keys[0].patterns, vec!["*"]);
    }
}
