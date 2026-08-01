#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUser {
    pub account_id: String,
    pub display_name: String,
    pub email: String,
    pub time_zone: String,
}
