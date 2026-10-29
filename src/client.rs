use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use reqwest::{Client, Method, StatusCode};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use anyhow::{anyhow, Result};

use crate::types::*;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_API_BASE_URL: &str = "https://pokepixel.nietore.com/api/v1";
const DEFAULT_CLIENT_VERSION: &str = "2026.07.26-sync-guard-v1";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct GameClient {
    base_url: String,
    http_client: Client,
    access_token: Arc<RwLock<String>>,
    signing_key: Arc<RwLock<String>>,
    client_version: String,
}

impl GameClient {
    pub fn new(initial_token: &str) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: DEFAULT_API_BASE_URL.to_string(),
            http_client,
            access_token: Arc::new(RwLock::new(initial_token.to_string())),
            signing_key: Arc::new(RwLock::new(String::new())),
            client_version: DEFAULT_CLIENT_VERSION.to_string(),
        }
    }

    pub async fn set_token(&self, token: &str) {
        let mut t = self.access_token.write().await;
        *t = token.to_string();
    }

    pub async fn get_token(&self) -> String {
        self.access_token.read().await.clone()
    }

    pub async fn set_signing_key(&self, key: &str) {
        let mut k = self.signing_key.write().await;
        *k = key.to_string();
    }

    fn sign_request(&self, key: &str, method: &str, path: &str, timestamp: i64, body_str: &str) -> Option<String> {
        if key.is_empty() {
            return None;
        }

        let mut hasher = Sha256::new();
        hasher.update(body_str.as_bytes());
        let digest = hex::encode(hasher.finalize());

        let clean_path = path.split('?').next().unwrap_or(path);
        let payload = format!("{}\n{}\n{}\n{}", method, clean_path, timestamp, digest);

        let mut mac = HmacSha256::new_from_slice(key.as_bytes()).ok()?;
        mac.update(payload.as_bytes());
        Some(hex::encode(mac.finalize().into_bytes()))
    }

    pub async fn do_request<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        endpoint: &str,
        body: Option<serde_json::Value>,
        idempotent: bool,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, endpoint);
        let token = self.get_token().await;
        let signing_key = self.signing_key.read().await.clone();

        let body_str = match &body {
            Some(b) => serde_json::to_string(b)?,
            None => String::new(),
        };

        let mut req = self.http_client.request(method.clone(), &url)
            .header("Content-Type", "application/json")
            .header("X-PokeIdle-Client", "web")
            .header("X-PokeIdle-Client-Version", &self.client_version)
            .header("Accept", "application/json, text/plain, */*")
            .header("User-Agent", DEFAULT_USER_AGENT);

        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        if idempotent || method == Method::POST || method == Method::PUT {
            req = req.header("Idempotency-Key", Uuid::new_v4().to_string());
        }

        if !signing_key.is_empty() && !token.is_empty() {
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            if let Some(sig) = self.sign_request(&signing_key, method.as_str(), endpoint, now_ms, &body_str) {
                req = req.header("X-PokeIdle-Timestamp", now_ms.to_string());
                req = req.header("X-PokeIdle-Signature", sig);
            }
        }

        if !body_str.is_empty() {
            req = req.body(body_str);
        }

        let resp = req.send().await?;
        let status = resp.status();
        let resp_bytes = resp.bytes().await?;

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(anyhow!("HTTP 429 Too Many Requests em {}", endpoint));
        }

        if !status.is_success() {
            let text = String::from_utf8_lossy(&resp_bytes);
            return Err(anyhow!("HTTP {} em {}: {}", status.as_u16(), endpoint, text));
        }

        if std::any::type_name::<T>() == "()" {
            return Ok(serde_json::from_str("null")?);
        }

        let parsed: T = serde_json::from_slice(&resp_bytes)
            .map_err(|e| anyhow!("Falha ao deserializar resposta de {}: {} (corpo: {})", endpoint, e, String::from_utf8_lossy(&resp_bytes)))?;

        Ok(parsed)
    }

    pub async fn login(&self, email: &str, pass: &str) -> Result<AuthLoginResponse> {
        let req_body = AuthLoginRequest {
            login: email.to_string(),
            password: pass.to_string(),
            captcha_token: String::new(),
        };

        let res: AuthLoginResponse = self.do_request(
            Method::POST,
            "/auth/login",
            Some(serde_json::to_value(req_body)?),
            false,
        ).await?;

        if !res.access_token.is_empty() {
            self.set_token(&res.access_token).await;
        }
        if let Some(ref key) = res.client_signing_key {
            if !key.is_empty() {
                self.set_signing_key(key).await;
            }
        }

        Ok(res)
    }

    pub async fn exchange_handoff(&self, code: &str) -> Result<AuthLoginResponse> {
        let req_body = serde_json::json!({ "code": code });
        let res: AuthLoginResponse = self.do_request(
            Method::POST,
            "/auth/handoff/exchange",
            Some(req_body),
            false,
        ).await?;

        if !res.access_token.is_empty() {
            self.set_token(&res.access_token).await;
        }
        if let Some(ref key) = res.client_signing_key {
            if !key.is_empty() {
                self.set_signing_key(key).await;
            }
        }

        Ok(res)
    }

    pub async fn get_me(&self) -> Result<Trainer> {
        let res: TrainerMeResponse = self.do_request(Method::GET, "/trainer/me", None, false).await?;
        Ok(res.trainer)
    }

    pub async fn get_creatures(&self, location: &str) -> Result<Vec<Creature>> {
        let endpoint = if location.is_empty() {
            "/creatures".to_string()
        } else {
            format!("/creatures?location={}", location)
        };

        let val: serde_json::Value = self.do_request(Method::GET, &endpoint, None, false).await?;
        if let Ok(list) = serde_json::from_value::<Vec<Creature>>(val.clone()) {
            return Ok(list);
        }
        if let Some(data) = val.get("data") {
            if let Ok(list) = serde_json::from_value::<Vec<Creature>>(data.clone()) {
                return Ok(list);
            }
        }
        if let Some(creatures) = val.get("creatures") {
            if let Ok(list) = serde_json::from_value::<Vec<Creature>>(creatures.clone()) {
                return Ok(list);
            }
        }
        Ok(Vec::new())
    }

    pub async fn get_zones(&self) -> Result<Vec<Zone>> {
        let val: serde_json::Value = self.do_request(Method::GET, "/zones", None, false).await?;
        if let Ok(list) = serde_json::from_value::<Vec<Zone>>(val.clone()) {
            return Ok(list);
        }
        if let Some(data) = val.get("data") {
            if let Ok(list) = serde_json::from_value::<Vec<Zone>>(data.clone()) {
                return Ok(list);
            }
        }
        if let Some(zones) = val.get("zones") {
            if let Ok(list) = serde_json::from_value::<Vec<Zone>>(zones.clone()) {
                return Ok(list);
            }
        }
        Ok(Vec::new())
    }

    pub async fn prepare_hunt(&self, zone_id: &str) -> Result<HuntAllocation> {
        let body = serde_json::json!({ "zone_id": zone_id });
        let res: PrepareHuntResponse = self.do_request(Method::POST, "/hunts/prepare", Some(body), true).await?;
        Ok(res.allocation)
    }

    pub async fn heal_team(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/team/heal", Some(serde_json::json!({})), false).await?;
        Ok(())
    }

    pub async fn claim_daily_login(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/daily-login/claim", Some(serde_json::json!({})), true).await?;
        Ok(())
    }

    pub async fn claim_all_quests(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/quests/claim-all", Some(serde_json::json!({})), true).await?;
        Ok(())
    }

    pub async fn claim_all_species_goals(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/species-goals/claim-all", Some(serde_json::json!({})), true).await?;
        Ok(())
    }

    pub async fn claim_all_battlepass(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/battlepass/claim-all", Some(serde_json::json!({})), true).await?;
        Ok(())
    }

    pub async fn claim_all_mail(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/mail/claim-all", Some(serde_json::json!({})), true).await?;
        Ok(())
    }

    pub async fn resolve_stamina(&self) -> Result<()> {
        let _: serde_json::Value = self.do_request(Method::POST, "/stamina/resolve", Some(serde_json::json!({})), false).await?;
        Ok(())
    }

    pub async fn collect_mining_rewards(&self, slot: i32) -> Result<()> {
        let body = serde_json::json!({ "slot_index": slot });
        let _: serde_json::Value = self.do_request(Method::POST, "/professions/mining/rewards/collect", Some(body), true).await?;
        Ok(())
    }

    pub async fn collect_farm_rewards(&self, slot: i32) -> Result<()> {
        let body = serde_json::json!({ "slot_index": slot });
        let _: serde_json::Value = self.do_request(Method::POST, "/professions/farming/rewards/collect", Some(body), true).await?;
        Ok(())
    }
}
