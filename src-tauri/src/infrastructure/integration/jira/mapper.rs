use super::dto::{JiraChangeHistoryDto, JiraIssueDto, JiraMyselfDto, JiraStatusListItemDto};
use crate::domain::changelog::IssueChange;
use crate::domain::current_user::CurrentUser;
use crate::domain::date_range::{format_utc, normalize_jira_timestamp};
use crate::domain::issue::{Issue, Status};
use chrono::{DateTime, Utc};

fn epoch_millis_to_utc(millis: i64) -> String {
    format_utc(DateTime::from_timestamp_millis(millis).unwrap_or_else(|| DateTime::<Utc>::default()))
}

pub fn map_myself(dto: JiraMyselfDto) -> CurrentUser {
    CurrentUser {
        account_id: dto.account_id,
        display_name: dto.display_name,
        email: dto.email_address.unwrap_or_default(),
        time_zone: dto.time_zone.unwrap_or_else(|| "America/Sao_Paulo".to_string()),
    }
}

pub fn map_issue(dto: JiraIssueDto, base_url: &str) -> (Issue, Status) {
    let browse_url = format!("{}/browse/{}", base_url.trim_end_matches('/'), &dto.key);

    let category_key = dto
        .fields
        .status
        .status_category
        .as_ref()
        .map(|c| c.key.clone())
        .unwrap_or_else(|| "indeterminate".to_string());

    let issue = Issue {
        id: dto.id.clone(),
        key: dto.key.clone(),
        summary: dto.fields.summary.clone(),
        status_id: dto.fields.status.id.clone(),
        status_name: dto.fields.status.name.clone(),
        status_category_key: category_key.clone(),
        assignee_account_id: dto.fields.assignee.as_ref().map(|a| a.account_id.clone()),
        assignee_display_name: dto.fields.assignee.as_ref().map(|a| a.display_name.clone()),
        priority_name: dto.fields.priority.as_ref().map(|p| p.name.clone()),
        issue_type_name: dto.fields.issue_type.as_ref().map(|t| t.name.clone()),
        due_date: dto.fields.due_date,
        browse_url,
        jira_updated_at: normalize_jira_timestamp(&dto.fields.updated),
    };

    let status = Status {
        id: dto.fields.status.id,
        name: dto.fields.status.name,
        untranslated_name: None,
        category_key: dto
            .fields
            .status
            .status_category
            .as_ref()
            .map(|c| c.key.clone())
            .unwrap_or_else(|| "indeterminate".to_string()),
        category_name: dto
            .fields
            .status
            .status_category
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "Unknown".to_string()),
        category_color: dto
            .fields
            .status
            .status_category
            .as_ref()
            .map(|c| c.color_name.clone())
            .unwrap_or_else(|| "default".to_string()),
        icon_url: None,
        is_development: false,
    };

    (issue, status)
}

pub fn map_statuses(dtos: &[JiraStatusListItemDto]) -> Vec<Status> {
    dtos.iter()
        .map(|dto| Status {
            id: dto.id.clone(),
            name: dto.name.clone(),
            untranslated_name: Some(dto.untranslated_name.clone()),
            category_key: dto.status_category.key.clone(),
            category_name: dto.status_category.name.clone(),
            category_color: dto.status_category.color_name.clone(),
            icon_url: Some(dto.icon_url.clone()),
            is_development: false,
        })
        .collect()
}

pub fn map_changelogs(histories: Vec<JiraChangeHistoryDto>) -> Vec<IssueChange> {
    let mut changes = Vec::new();

    for history in histories {
        let jira_created_at = epoch_millis_to_utc(history.created);

        for (item_index, item) in history.items.into_iter().enumerate() {
            let field_id = item.field_id.clone().unwrap_or_else(|| item.field.clone());
            if field_id != "status" && field_id != "assignee" {
                continue;
            }

            changes.push(IssueChange {
                changelog_id: history.id.clone(),
                item_index: item_index as i32,
                author_account_id: history.author.as_ref().map(|a| a.account_id.clone()),
                author_display_name: history.author.as_ref().map(|a| a.display_name.clone()),
                jira_created_at: jira_created_at.clone(),
                field_id: Some(field_id),
                field: item.field,
                field_type: item.field_type,
                from_id: item.from,
                from_string: item.from_string,
                to_id: item.to,
                to_string: item.to_string,
            });
        }
    }

    changes
}
