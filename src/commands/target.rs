use crate::modules::{
    common::utils,
    lister::{
        self,
        models::{InstallSourceSelector, InstalledProgram, ListProgramsQuery},
        storage,
    },
};
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct ResolvedTarget {
    pub program: InstalledProgram,
}

pub fn build_target_search_query() -> ListProgramsQuery {
    ListProgramsQuery {
        source: InstallSourceSelector::All,
        search: None,
        refresh: true,
        cache_ttl_seconds: storage::DEFAULT_CACHE_TTL_SECONDS,
    }
}

pub fn resolve_installed_target(target: &str) -> Result<ResolvedTarget> {
    let programs = lister::list_programs_with_cache(build_target_search_query())?.programs;
    resolve_target_from_programs(programs, target)
}

pub fn resolve_target_from_programs(
    programs: Vec<InstalledProgram>,
    target: &str,
) -> Result<ResolvedTarget> {
    let target = target.trim();
    if target.is_empty() {
        bail!("请输入要操作的 App 名称或程序 ID");
    }

    let exact_id_matches = programs
        .iter()
        .filter(|program| program.id.eq_ignore_ascii_case(target))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(program) = pick_unique_match(exact_id_matches, target)? {
        return Ok(ResolvedTarget { program });
    }

    let normalized_target = utils::normalize_search_text(target);
    let exact_name_matches = programs
        .iter()
        .filter(|program| utils::normalize_search_text(&program.name) == normalized_target)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(program) = pick_unique_match(exact_name_matches, target)? {
        return Ok(ResolvedTarget { program });
    }

    let partial_matches = programs
        .into_iter()
        .filter(|program| utils::fuzzy_match(&program.name, target))
        .collect::<Vec<_>>();
    if let Some(program) = pick_unique_match(partial_matches, target)? {
        return Ok(ResolvedTarget { program });
    }

    bail!(
        "未找到匹配的 App: {target}\n请先使用 `yu list --search \"{target}\"` 确认精确名称或程序 ID"
    )
}

pub fn format_program_choice(program: &InstalledProgram) -> String {
    format!(
        "{} | Publisher: {} | Version: {} | Source: {} | ID: {}",
        program.name,
        program.publisher.as_deref().unwrap_or("-"),
        program.version.as_deref().unwrap_or("-"),
        program.install_source,
        program.id
    )
}

pub fn format_selected_target(program: &InstalledProgram) -> String {
    let mut lines = vec![
        format!("  - 名称: {}", program.name),
        format!("  - 来源: {}", program.install_source),
        format!("  - ID: {}", program.id),
    ];

    if let Some(publisher) = program.publisher.as_deref() {
        lines.push(format!("  - 发布者: {publisher}"));
    }
    if let Some(version) = program.version.as_deref() {
        lines.push(format!("  - 版本: {version}"));
    }
    if let Some(location) = program.install_location.as_deref() {
        lines.push(format!("  - 安装位置: {location}"));
    }

    lines.join("\n")
}

fn pick_unique_match(
    matches: Vec<InstalledProgram>,
    target: &str,
) -> Result<Option<InstalledProgram>> {
    match matches.as_slice() {
        [] => Ok(None),
        [program] => Ok(Some(program.clone())),
        _ => bail!(
            "找到多个匹配的 App，请使用更精确的名称或程序 ID:\n{}",
            format_program_candidates(&matches, target)
        ),
    }
}

fn format_program_candidates(programs: &[InstalledProgram], target: &str) -> String {
    let mut candidates = Vec::with_capacity(programs.len() + 1);
    candidates.push(format!("查询: {target}"));
    candidates.extend(
        programs
            .iter()
            .enumerate()
            .map(|(index, program)| format!("{}. {}", index + 1, format_program_choice(program))),
    );
    candidates.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{build_target_search_query, format_program_choice, resolve_target_from_programs};
    use crate::modules::lister::models::{InstallSource, InstallSourceSelector, InstalledProgram};

    fn make_program(
        id: &str,
        name: &str,
        publisher: Option<&str>,
        version: Option<&str>,
        source: InstallSource,
    ) -> InstalledProgram {
        let mut program = InstalledProgram::new(name.to_string(), source);
        program.id = id.to_string();
        program.publisher = publisher.map(str::to_string);
        program.version = version.map(str::to_string);
        program
    }

    #[test]
    fn build_target_search_query_refreshes_all_sources() {
        let query = build_target_search_query();

        assert_eq!(query.source, InstallSourceSelector::All);
        assert!(query.refresh);
        assert!(query.search.is_none());
    }

    #[test]
    fn resolve_target_from_programs_prefers_exact_id_match() {
        let first = make_program(
            "7zip-id",
            "7-Zip 24.09 (x64)",
            Some("Igor Pavlov"),
            Some("24.09"),
            InstallSource::Registry,
        );
        let second = make_program(
            "chatgpt-id",
            "ChatGPT",
            Some("OpenAI"),
            Some("1.0"),
            InstallSource::Store,
        );

        let resolved = resolve_target_from_programs(vec![first, second.clone()], "chatgpt-id")
            .expect("应优先按程序 ID 精确匹配");

        assert_eq!(resolved.program.id, second.id);
    }

    #[test]
    fn resolve_target_from_programs_prefers_exact_name_match_over_partial_match() {
        let exact = make_program(
            "7zip-exact",
            "7-Zip",
            Some("Igor Pavlov"),
            Some("24.09"),
            InstallSource::Registry,
        );
        let partial = make_program(
            "7zip-other",
            "7-Zip Helper",
            Some("Igor Pavlov"),
            Some("1.0"),
            InstallSource::Registry,
        );

        let resolved = resolve_target_from_programs(vec![partial, exact.clone()], "7-Zip")
            .expect("精确名称匹配应优先于模糊匹配");

        assert_eq!(resolved.program.id, exact.id);
    }

    #[test]
    fn resolve_target_from_programs_rejects_ambiguous_partial_matches() {
        let first = make_program(
            "7zip-a",
            "7-Zip 24.09 (x64)",
            Some("Igor Pavlov"),
            Some("24.09"),
            InstallSource::Registry,
        );
        let second = make_program(
            "7zip-b",
            "7-Zip 24.01 (x86)",
            Some("Igor Pavlov"),
            Some("24.01"),
            InstallSource::Registry,
        );

        let error = resolve_target_from_programs(vec![first, second], "7-Zip")
            .expect_err("模糊匹配出多个 App 时应拒绝继续");

        let message = error.to_string();
        assert!(message.contains("找到多个匹配的 App"));
        assert!(message.contains("7-Zip 24.09 (x64)"));
        assert!(message.contains("7-Zip 24.01 (x86)"));
    }

    #[test]
    fn format_program_choice_includes_disambiguation_fields() {
        let program = make_program(
            "chatgpt-id",
            "ChatGPT",
            Some("OpenAI"),
            Some("1.0"),
            InstallSource::Store,
        );

        let rendered = format_program_choice(&program);

        assert!(rendered.contains("ChatGPT"));
        assert!(rendered.contains("OpenAI"));
        assert!(rendered.contains("1.0"));
        assert!(rendered.contains("Store"));
        assert!(rendered.contains("chatgpt-id"));
    }
}
