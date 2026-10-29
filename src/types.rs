use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default, rename = "bindIP")]
    pub bind_ip: String,
    pub credentials: GameCredentials,
    pub settings: GameSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameCredentials {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub token: String,
    #[serde(default, rename = "handoffCode")]
    pub handoff_code: String,
    #[serde(default, rename = "sessionCookie")]
    pub session_cookie: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    #[serde(default = "default_hunt_target", rename = "huntTarget")]
    pub hunt_target: String,
    #[serde(default = "default_true", rename = "autoHunt")]
    pub auto_hunt: bool,
    #[serde(default = "default_true", rename = "autoHeal")]
    pub auto_heal: bool,
    #[serde(default = "default_hp_threshold", rename = "hpThreshold")]
    pub hp_threshold: u32,
    #[serde(default = "default_true", rename = "autoCapture")]
    pub auto_capture: bool,
    #[serde(default = "default_true", rename = "autoCaptureShiny")]
    pub auto_capture_shiny: bool,
    #[serde(default = "default_true", rename = "autoSell")]
    pub auto_sell: bool,
    #[serde(default = "default_true", rename = "autoExtract")]
    pub auto_extract: bool,
    #[serde(default = "default_true", rename = "autoDaily")]
    pub auto_daily: bool,
    #[serde(default = "default_true", rename = "autoProfessions")]
    pub auto_professions: bool,
    #[serde(default = "default_true", rename = "autoBreeding")]
    pub auto_breeding: bool,
    #[serde(default, rename = "bestiaryFocus")]
    pub bestiary_focus: bool,
    #[serde(default = "default_true", rename = "stealthMode")]
    pub stealth_mode: bool,
}

fn default_hunt_target() -> String { "auto".to_string() }
fn default_true() -> bool { true }
fn default_hp_threshold() -> u32 { 40 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginRequest {
    pub login: String,
    pub password: String,
    #[serde(default)]
    pub captcha_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthLoginResponse {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub client_signing_key: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainerMeResponse {
    pub trainer: Trainer,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Trainer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub exp: i64,
    #[serde(default)]
    pub gold: i64,
    #[serde(default)]
    pub diamonds: i64,
    #[serde(default)]
    pub vocation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Creature {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub species_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub hp: u32,
    #[serde(default)]
    pub max_hp: u32,
    #[serde(default)]
    pub exp: i64,
    #[serde(default)]
    pub next_exp: i64,
    #[serde(default)]
    pub elements: Vec<String>,
    #[serde(default)]
    pub is_leader: bool,
    #[serde(default)]
    pub is_shiny: bool,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Zone {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub world: Option<String>,
    #[serde(default)]
    pub recommended_level: u32,
    #[serde(default)]
    pub min_level: u32,
    #[serde(default)]
    pub max_level: u32,
    #[serde(default)]
    pub theme_element: Option<String>,
    #[serde(default)]
    pub map_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrepareHuntResponse {
    pub allocation: HuntAllocation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HuntAllocation {
    #[serde(default)]
    pub zone_id: String,
    #[serde(default)]
    pub zone_name: Option<String>,
    #[serde(default)]
    pub map_id: i32,
    #[serde(default)]
    pub species_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountStatusDto {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub vocation: String,
    pub hp: u32,
    #[serde(rename = "maxHp")]
    pub max_hp: u32,
    pub mana: u32,
    #[serde(rename = "maxMana")]
    pub max_mana: u32,
    pub gold: i64,
    pub kills: u64,
    #[serde(rename = "expPerHour")]
    pub exp_per_hour: f64,
    pub status: String,
    #[serde(rename = "currentArea")]
    pub current_area: String,
    #[serde(rename = "durationSec")]
    pub duration_sec: u64,
    pub logs: Vec<String>,
    #[serde(rename = "updatedAt")]
    pub updated_at: i64,
    pub extra: ExtraStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtraStatusDto {
    #[serde(rename = "bestiaryFocus")]
    pub bestiary_focus: bool,
    #[serde(rename = "circuitState")]
    pub circuit_state: String,
    #[serde(rename = "cooldownSec")]
    pub cooldown_sec: u64,
    #[serde(rename = "leaderLevel")]
    pub leader_level: u32,
    #[serde(rename = "leaderName")]
    pub leader_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryResponse {
    pub accounts: Vec<AccountStatusDto>,
    pub goroutines: u32,
    #[serde(rename = "ramAllocMB")]
    pub ram_alloc_mb: f64,
    #[serde(rename = "ramSysMB")]
    pub ram_sys_mb: f64,
    pub uptime: String,
}
