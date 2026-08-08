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

    bail!("未找到匹配的 App: {target}\n请使用精确名称或程序 ID 确认目标")
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
    use super::{resolve_target_from_programs, ResolvedTarget};
    use crate::modules::lister::models::{InstallSource, InstalledProgram};

    fn make_program(id: &str, name: &str, source: InstallSource) -> InstalledProgram {
        let mut program = InstalledProgram::new(name.to_string(), source);
        program.id = id.to_string();
        program
    }

    fn resolve(programs: Vec<InstalledProgram>, target: &str) -> ResolvedTarget {
        resolve_target_from_programs(programs, target).expect("测试目标应能被精确解析")
    }

    #[test]
    fn exact_id_match_is_preferred() {
        let selected = resolve(
            vec![
                make_program("one", "Demo", InstallSource::Registry),
                make_program("two", "Other", InstallSource::Store),
            ],
            "two",
        );
        assert_eq!(selected.program.id, "two");
    }

    #[test]
    fn ambiguous_name_is_rejected() {
        let error = resolve_target_from_programs(
            vec![
                make_program("one", "Demo x64", InstallSource::Registry),
                make_program("two", "Demo x86", InstallSource::Registry),
            ],
            "Demo",
        )
        .expect_err("模糊匹配不能自动选择目标");
        assert!(error.to_string().contains("找到多个匹配的 App"));
    }
}
