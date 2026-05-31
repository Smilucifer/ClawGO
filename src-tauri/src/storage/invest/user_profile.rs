use super::with_conn;
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub emergency_buffer_cny: f64,
    pub family_backup_available: bool,
    pub account_purpose: String,
    pub lifestyle_notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_tolerance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exchange_buffer_cny: Option<f64>,
    /// Family economic support level: "none", "partial", "full", "occasional".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_support: Option<String>,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            emergency_buffer_cny: 100_000.0,
            family_backup_available: false,
            account_purpose: "default".into(),
            lifestyle_notes: String::new(),
            display_name: None,
            risk_tolerance: None,
            exchange_buffer_cny: None,
            family_support: None,
        }
    }
}

pub fn get_profile() -> Result<Option<UserProfile>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT emergency_buffer_cny, family_backup_available, account_purpose, \
                 lifestyle_notes, display_name, risk_tolerance, exchange_buffer_cny, family_support \
                 FROM user_profile WHERE id = 1",
            )
            .map_err(|e| format!("prepare get_profile: {e}"))?;

        let result = stmt.query_row([], |row| {
            Ok(UserProfile {
                emergency_buffer_cny: row.get(0)?,
                family_backup_available: row.get::<_, i64>(1)? != 0,
                account_purpose: row.get(2)?,
                lifestyle_notes: row.get(3)?,
                display_name: row.get(4)?,
                risk_tolerance: row.get(5)?,
                exchange_buffer_cny: row.get(6)?,
                family_support: row.get(7)?,
            })
        });

        match result {
            Ok(profile) => Ok(Some(profile)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("get_profile: {e}")),
        }
    })
}

pub fn save_profile(profile: &UserProfile) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO user_profile (id, emergency_buffer_cny, family_backup_available, \
             account_purpose, lifestyle_notes, display_name, risk_tolerance, exchange_buffer_cny, \
             family_support, updated_at) \
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now')) \
             ON CONFLICT(id) DO UPDATE SET \
             emergency_buffer_cny = excluded.emergency_buffer_cny, \
             family_backup_available = excluded.family_backup_available, \
             account_purpose = excluded.account_purpose, \
             lifestyle_notes = excluded.lifestyle_notes, \
             display_name = excluded.display_name, \
             risk_tolerance = excluded.risk_tolerance, \
             exchange_buffer_cny = excluded.exchange_buffer_cny, \
             family_support = excluded.family_support, \
             updated_at = excluded.updated_at",
            params![
                profile.emergency_buffer_cny,
                profile.family_backup_available as i64,
                profile.account_purpose,
                profile.lifestyle_notes,
                profile.display_name,
                profile.risk_tolerance,
                profile.exchange_buffer_cny,
                profile.family_support,
            ],
        )
        .map_err(|e| format!("save_profile: {e}"))?;
        Ok(())
    })
}
