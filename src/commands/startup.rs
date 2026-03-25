use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};

use crate::modules::startup::manager;
use crate::modules::startup::models::{
    StartupAction, StartupEnvelope, StartupError, StartupItem, StartupListQuery, StartupScope,
    StartupSource, StartupState,
};

#[derive(Parser, Debug)]
pub struct StartupCommand {
    #[command(subcommand)]
    pub command: StartupSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum StartupSubcommand {
    /// 列出自启动项
    List(ListStartupCommand),
    /// 查看单个自启动项
    Show(ShowStartupCommand),
    /// 查看支持的来源
    Sources(SourcesStartupCommand),
    /// 启用自启动项
    Enable(MutateStartupCommand),
    /// 禁用自启动项
    Disable(MutateStartupCommand),
    /// 删除自启动项
    Delete(MutateStartupCommand),
    /// 回滚已执行的变更
    Rollback(RollbackStartupCommand),
}

#[derive(Parser, Debug)]
pub struct ListStartupCommand {
    #[arg(long, default_value = "table")]
    pub format: String,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long, default_value = "all")]
    pub scope: String,
    #[arg(long, default_value = "all")]
    pub state: String,
    #[arg(long)]
    pub search: Option<String>,
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
    #[arg(long, default_value = "name")]
    pub sort_by: String,
    #[arg(long)]
    pub descending: bool,
    #[arg(long)]
    pub include_raw: bool,
    #[arg(long, value_delimiter = ',')]
    pub fields: Vec<String>,
}

#[derive(Parser, Debug)]
pub struct ShowStartupCommand {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "json")]
    pub format: String,
    #[arg(long)]
    pub include_raw: bool,
    #[arg(long, value_delimiter = ',')]
    pub fields: Vec<String>,
}

#[derive(Parser, Debug)]
pub struct SourcesStartupCommand {
    #[arg(long, default_value = "table")]
    pub format: String,
}

#[derive(Parser, Debug)]
pub struct MutateStartupCommand {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "json")]
    pub format: String,
    #[arg(long)]
    pub apply: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Parser, Debug)]
pub struct RollbackStartupCommand {
    #[arg(long)]
    pub change_id: String,
    #[arg(long, default_value = "json")]
    pub format: String,
    #[arg(long)]
    pub apply: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub reason: Option<String>,
}

pub async fn execute(cmd: StartupCommand) -> Result<()> {
    match cmd.command {
        StartupSubcommand::List(command) => execute_list(command),
        StartupSubcommand::Show(command) => execute_show(command),
        StartupSubcommand::Sources(command) => execute_sources(command),
        StartupSubcommand::Enable(command) => execute_mutation(command, StartupAction::Enable),
        StartupSubcommand::Disable(command) => execute_mutation(command, StartupAction::Disable),
        StartupSubcommand::Delete(command) => execute_mutation(command, StartupAction::Delete),
        StartupSubcommand::Rollback(command) => execute_rollback(command),
    }
}

fn execute_list(cmd: ListStartupCommand) -> Result<()> {
    let query = StartupListQuery {
        source: parse_source_filter(cmd.source.as_deref())?,
        scope: parse_scope_filter(&cmd.scope)?,
        state: parse_state_filter(&cmd.state)?,
        search: cmd.search.clone(),
        limit: cmd.limit,
        offset: cmd.offset,
        sort_by: Some(cmd.sort_by.clone()),
        descending: cmd.descending,
        include_raw: cmd.include_raw,
    };

    match manager::list_startup_items(query) {
        Ok(response) => {
            if cmd.format.eq_ignore_ascii_case("json") {
                let mut value =
                    serde_json::to_value(response).unwrap_or_else(|_| serde_json::json!({}));
                apply_field_projection_to_items(&mut value, &cmd.fields);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&StartupEnvelope::success(value))?
                );
            } else if let Some(items) = serde_json::to_value(response)
                .ok()
                .and_then(|value| value.get("items").cloned())
                .and_then(|value| serde_json::from_value::<Vec<StartupItem>>(value).ok())
            {
                print_items_table(&items);
            }
            Ok(())
        }
        Err(error) => render_error(&cmd.format, error),
    }
}

fn execute_show(cmd: ShowStartupCommand) -> Result<()> {
    match manager::get_startup_item(&cmd.id, cmd.include_raw) {
        Ok(item) => {
            let mut value = serde_json::to_value(item).unwrap_or_else(|_| serde_json::json!({}));
            apply_field_projection(&mut value, &cmd.fields);
            if cmd.format.eq_ignore_ascii_case("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&StartupEnvelope::success(value))?
                );
            } else {
                print_item_detail(&value);
            }
            Ok(())
        }
        Err(error) => render_error(&cmd.format, error),
    }
}

fn execute_sources(cmd: SourcesStartupCommand) -> Result<()> {
    let response = manager::list_sources();
    if cmd.format.eq_ignore_ascii_case("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&StartupEnvelope::success(response))?
        );
    } else {
        println!(
            "{:<20} {:<10} {:<10} {:<10} 说明",
            "来源", "用户", "机器", "删除"
        );
        for item in response {
            println!(
                "{:<20} {:<10} {:<10} {:<10} {}",
                item.label,
                yes_no(item.supports_user_scope),
                yes_no(item.supports_machine_scope),
                yes_no(item.capabilities.can_delete),
                item.notes
            );
        }
    }
    Ok(())
}

fn execute_mutation(cmd: MutateStartupCommand, action: StartupAction) -> Result<()> {
    if !cmd.apply {
        match manager::plan_action(&cmd.id, action, cmd.reason.clone(), false) {
            Ok(plan) => {
                if cmd.format.eq_ignore_ascii_case("json") {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&StartupEnvelope::success(plan))?
                    );
                } else {
                    print_action_plan(&plan);
                }
                return Ok(());
            }
            Err(error) => return render_error(&cmd.format, error),
        }
    }

    if !cmd.yes && !confirm_action(action, &cmd.id)? {
        let envelope = StartupEnvelope::<serde_json::Value>::failure("conflict", "操作已取消");
        if cmd.format.eq_ignore_ascii_case("json") {
            println!("{}", serde_json::to_string_pretty(&envelope)?);
            return Ok(());
        }
        return Err(anyhow!("操作已取消"));
    }

    match manager::apply_action(&cmd.id, action, cmd.reason.clone()) {
        Ok(result) => {
            if cmd.format.eq_ignore_ascii_case("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&StartupEnvelope::success(result))?
                );
            } else {
                print_action_result(&result);
            }
            Ok(())
        }
        Err(error) => render_error(&cmd.format, error),
    }
}

fn execute_rollback(cmd: RollbackStartupCommand) -> Result<()> {
    if !cmd.apply {
        let plan = serde_json::json!({
            "change_id": cmd.change_id,
            "action": "rollback",
            "will_apply": false,
            "operations": ["读取变更日志", "恢复来源状态", "标记变更已回滚"],
        });
        if cmd.format.eq_ignore_ascii_case("json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&StartupEnvelope::success(plan))?
            );
        } else {
            print_item_detail(&plan);
        }
        return Ok(());
    }

    if !cmd.yes && !confirm_action(StartupAction::Rollback, &cmd.change_id)? {
        let envelope = StartupEnvelope::<serde_json::Value>::failure("conflict", "操作已取消");
        if cmd.format.eq_ignore_ascii_case("json") {
            println!("{}", serde_json::to_string_pretty(&envelope)?);
            return Ok(());
        }
        return Err(anyhow!("操作已取消"));
    }

    match manager::rollback_action(&cmd.change_id, cmd.reason.clone()) {
        Ok(result) => {
            if cmd.format.eq_ignore_ascii_case("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&StartupEnvelope::success(result))?
                );
            } else {
                print_action_result(&result);
            }
            Ok(())
        }
        Err(error) => render_error(&cmd.format, error),
    }
}

fn render_error(format: &str, error: StartupError) -> Result<()> {
    if format.eq_ignore_ascii_case("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &StartupEnvelope::<serde_json::Value>::from_startup_error(&error)
            )?
        );
        return Ok(());
    }

    Err(anyhow!("{}", error.message))
}

fn parse_source_filter(source: Option<&str>) -> Result<Option<StartupSource>> {
    match source.map(|value| value.to_lowercase()) {
        None => Ok(None),
        Some(value) if value == "all" => Ok(None),
        Some(value) if value == "registry_run" || value == "run" => {
            Ok(Some(StartupSource::RegistryRun))
        }
        Some(value) if value == "registry_run_once" || value == "run_once" => {
            Ok(Some(StartupSource::RegistryRunOnce))
        }
        Some(value) if value == "registry_policy_run" || value == "policy_run" => {
            Ok(Some(StartupSource::RegistryPolicyRun))
        }
        Some(value) if value == "startup_folder" || value == "folder" => {
            Ok(Some(StartupSource::StartupFolder))
        }
        Some(value) if value == "scheduled_task" || value == "task" => {
            Ok(Some(StartupSource::ScheduledTask))
        }
        Some(value) if value == "service" || value == "services" => {
            Ok(Some(StartupSource::Service))
        }
        Some(value) => Err(anyhow!("未知来源: {value}")),
    }
}

fn parse_scope_filter(scope: &str) -> Result<Option<StartupScope>> {
    match scope.to_lowercase().as_str() {
        "all" => Ok(None),
        "user" => Ok(Some(StartupScope::User)),
        "machine" => Ok(Some(StartupScope::Machine)),
        other => Err(anyhow!("未知作用域: {other}")),
    }
}

fn parse_state_filter(state: &str) -> Result<Option<StartupState>> {
    match state.to_lowercase().as_str() {
        "all" => Ok(None),
        "enabled" => Ok(Some(StartupState::Enabled)),
        "disabled" => Ok(Some(StartupState::Disabled)),
        "broken" => Ok(Some(StartupState::Broken)),
        other => Err(anyhow!("未知状态: {other}")),
    }
}

fn print_items_table(items: &[StartupItem]) {
    println!(
        "{:<18} {:<12} {:<8} {:<10} {:<8} 名称",
        "ID", "来源", "作用域", "状态", "提权"
    );
    for item in items {
        println!(
            "{:<18} {:<12} {:<8} {:<10} {:<8} {}",
            shorten(&item.id, 18),
            item.source.as_str(),
            item.scope.as_str(),
            format!("{:?}", item.state).to_lowercase(),
            yes_no(item.requires_admin),
            item.name
        );
    }
    println!("总计: {}", items.len());
}

fn print_item_detail(value: &serde_json::Value) {
    if let Ok(text) = serde_json::to_string_pretty(value) {
        println!("{text}");
    }
}

fn print_action_plan(plan: &crate::modules::startup::models::StartupActionPlan) {
    println!("项目: {}", plan.item_id);
    println!("动作: {}", plan.action.as_str());
    println!("需要管理员: {}", yes_no(plan.requires_admin));
    println!("预演步骤:");
    for operation in &plan.operations {
        println!("  - {operation}");
    }
}

fn print_action_result(result: &crate::modules::startup::models::StartupActionResult) {
    println!("动作: {}", result.action.as_str());
    println!("已执行: {}", yes_no(result.applied));
    if let Some(change_id) = &result.change_id {
        println!("变更 ID: {change_id}");
    }
    for operation in &result.operations {
        println!("  - {operation}");
    }
}

fn confirm_action(action: StartupAction, target: &str) -> Result<bool> {
    use std::io::{self, Write};

    print!("确认对 {target} 执行 {}? [y/N]: ", action.as_str());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn shorten(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }

    let prefix: String = input.chars().take(max_len.saturating_sub(2)).collect();
    format!("{prefix}..")
}

fn apply_field_projection_to_items(value: &mut serde_json::Value, fields: &[String]) {
    if fields.is_empty() {
        return;
    }

    if let Some(items) = value
        .get_mut("items")
        .and_then(|items| items.as_array_mut())
    {
        for item in items {
            apply_field_projection(item, fields);
        }
    }
}

fn apply_field_projection(value: &mut serde_json::Value, fields: &[String]) {
    if fields.is_empty() {
        return;
    }

    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.retain(|key, _| fields.iter().any(|field| field == key));
}

#[cfg(test)]
mod tests {
    use super::StartupCommand;
    use clap::Parser;

    #[test]
    fn startup_list_command_accepts_json_flags() {
        let parsed = StartupCommand::try_parse_from([
            "yu",
            "list",
            "--format",
            "json",
            "--source",
            "service",
            "--scope",
            "machine",
            "--state",
            "disabled",
            "--fields",
            "id,name,state",
        ]);

        assert!(parsed.is_ok(), "expected startup list flags to parse");
    }

    #[test]
    fn startup_disable_command_accepts_apply_and_yes() {
        let parsed = StartupCommand::try_parse_from([
            "yu",
            "disable",
            "--id",
            "startup:demo",
            "--apply",
            "--yes",
            "--reason",
            "test",
        ]);

        assert!(parsed.is_ok(), "expected startup disable flags to parse");
    }
}
