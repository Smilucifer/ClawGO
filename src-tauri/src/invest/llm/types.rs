use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Provider identity
// ---------------------------------------------------------------------------
//
// Retained after the OpenAiCompatClient/llm_config.json removal (Task C4) for
// archival labelling only — the committee orchestrator stamps the
// `verdicts.provider`/`verdicts.model` columns from this enum at archive time.
// All actual provider routing now flows through the CLI executor + the
// `--settings` JSON written by `write_committee_settings_json`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    DeepSeek,
    MiMoPlan,
    MiMoApi,
}

impl ProviderId {
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek-v4-pro",
            Self::MiMoPlan => "mimo-v2.5-pro",
            Self::MiMoApi => "mimo-v2.5-pro",
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeepSeek => write!(f, "DeepSeek"),
            Self::MiMoPlan => write!(f, "MiMo Plan"),
            Self::MiMoApi => write!(f, "MiMo API"),
        }
    }
}
