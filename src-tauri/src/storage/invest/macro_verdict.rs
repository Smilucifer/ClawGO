use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::Connection;

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS macro_verdict (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    signal                TEXT,
    strength              REAL,
    market_phase          TEXT,
    money_effect          TEXT,
    money_effect_reason   TEXT,
    signal_reason         TEXT,
    market_phase_reason   TEXT,
    based_on_data_version TEXT NOT NULL,
    updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
);";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroVerdict {
    pub signal: Option<String>,
    pub strength: Option<f64>,
    pub market_phase: Option<String>,
    pub money_effect: Option<String>,
    pub money_effect_reason: Option<String>,
    pub signal_reason: Option<String>,
    pub market_phase_reason: Option<String>,
    pub based_on_data_version: String,
    pub updated_at: String,
}

/// 建表(init_db 阶段调用,静态 DB 尚未就绪)。
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create macro_verdict table: {e}"))
}

/// 判断是否仍对应当前数据版本(纯函数,可测)。
pub fn is_current(v: &MacroVerdict, current_version: &str) -> bool {
    v.based_on_data_version == current_version
}

/// 保存全局宏观判断(id=1 单行 upsert)。
pub fn save_verdict(v: &MacroVerdict) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO macro_verdict (id, signal, strength, market_phase, money_effect,
                 money_effect_reason, signal_reason, market_phase_reason, based_on_data_version, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 signal=excluded.signal, strength=excluded.strength, market_phase=excluded.market_phase,
                 money_effect=excluded.money_effect, money_effect_reason=excluded.money_effect_reason,
                 signal_reason=excluded.signal_reason, market_phase_reason=excluded.market_phase_reason,
                 based_on_data_version=excluded.based_on_data_version, updated_at=excluded.updated_at",
            rusqlite::params![
                v.signal, v.strength, v.market_phase, v.money_effect,
                v.money_effect_reason, v.signal_reason, v.market_phase_reason,
                v.based_on_data_version
            ],
        )
        .map_err(|e| format!("save macro_verdict: {e}"))?;
        Ok(())
    })
}

/// 加载全局宏观判断。
pub fn load_verdict() -> Result<Option<MacroVerdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT signal, strength, market_phase, money_effect, money_effect_reason,
                        signal_reason, market_phase_reason, based_on_data_version, updated_at
                 FROM macro_verdict WHERE id = 1",
            )
            .map_err(|e| format!("prepare load_verdict: {e}"))?;
        let mut rows = stmt
            .query_map([], |r| {
                Ok(MacroVerdict {
                    signal: r.get(0)?,
                    strength: r.get(1)?,
                    market_phase: r.get(2)?,
                    money_effect: r.get(3)?,
                    money_effect_reason: r.get(4)?,
                    signal_reason: r.get(5)?,
                    market_phase_reason: r.get(6)?,
                    based_on_data_version: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })
            .map_err(|e| format!("query load_verdict: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read load_verdict: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_current() {
        let v = MacroVerdict {
            based_on_data_version: "b:X|m:Y".into(),
            ..Default::default()
        };
        assert!(is_current(&v, "b:X|m:Y"));
        assert!(!is_current(&v, "b:X2|m:Y"));
    }
}
