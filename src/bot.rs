use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::client::GameClient;
use crate::types::*;

pub struct BotState {
    pub level: u32,
    pub hp: u32,
    pub max_hp: u32,
    pub mana: u32,
    pub max_mana: u32,
    pub gold: i64,
    pub kills: u64,
    pub exp_per_hour: f64,
    pub status: String,
    pub current_area: String,
    pub logs: VecDeque<String>,
    pub start_time: Instant,
    pub start_exp: i64,
    pub current_exp: i64,
    pub is_hunting: bool,
    pub current_map_id: i32,
    pub current_zone_id: String,
    pub zones: Vec<Zone>,
    pub leader_name: String,
    pub leader_level: u32,
    pub circuit_state: String,
}

impl BotState {
    pub fn new() -> Self {
        Self {
            level: 1,
            hp: 100,
            max_hp: 100,
            mana: 0,
            max_mana: 0,
            gold: 0,
            kills: 0,
            exp_per_hour: 0.0,
            status: "Iniciando...".to_string(),
            current_area: "Iniciando...".to_string(),
            logs: VecDeque::with_capacity(30),
            start_time: Instant::now(),
            start_exp: 0,
            current_exp: 0,
            is_hunting: false,
            current_map_id: 0,
            current_zone_id: String::new(),
            zones: Vec::new(),
            leader_name: String::new(),
            leader_level: 1,
            circuit_state: "CLOSED (Operação Normal)".to_string(),
        }
    }

    pub fn add_log(&mut self, msg: String) {
        let now = chrono_like_now();
        let formatted = format!("[{}] {}", now, msg);
        info!("{}", formatted);
        if self.logs.len() >= 25 {
            self.logs.pop_front();
        }
        self.logs.push_back(formatted);
    }
}

fn chrono_like_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

#[derive(Clone)]
pub struct PokePixelBot {
    pub config: AccountConfig,
    pub client: GameClient,
    pub state: Arc<RwLock<BotState>>,
    pub ws_sender: Arc<RwLock<Option<mpsc::Sender<String>>>>,
}

impl PokePixelBot {
    pub fn new(config: AccountConfig) -> Self {
        let client = GameClient::new(&config.credentials.token);
        Self {
            config,
            client,
            state: Arc::new(RwLock::new(BotState::new())),
            ws_sender: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn add_log(&self, msg: String) {
        let mut st = self.state.write().await;
        st.add_log(msg);
    }

    pub async fn send_ws_message(&self, msg: String) -> bool {
        let guard = self.ws_sender.read().await;
        if let Some(ref tx) = *guard {
            tx.send(msg).await.is_ok()
        } else {
            false
        }
    }

    pub async fn init(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut st = self.state.write().await;
            st.status = "Autenticando...".to_string();
            st.current_area = self.config.settings.hunt_target.clone();
            st.add_log("Inicializando cliente PokePixel Rust...".to_string());
        }

        // 1. Autenticação
        let mut token = self.client.get_token().await;
        if token.is_empty() {
            if !self.config.credentials.handoff_code.is_empty() {
                self.add_log(format!("Trocando código de handoff ({}) por sessão...", self.config.credentials.handoff_code)).await;
                if let Ok(res) = self.client.exchange_handoff(&self.config.credentials.handoff_code).await {
                    self.add_log("Token de sessão obtido com sucesso via Handoff!".to_string()).await;
                    token = res.access_token;
                }
            }

            if token.is_empty() && !self.config.credentials.email.is_empty() && !self.config.credentials.password.is_empty() {
                self.add_log(format!("Efetuando login para {}...", self.config.credentials.email)).await;
                match self.client.login(&self.config.credentials.email, &self.config.credentials.password).await {
                    Ok(_) => {
                        self.add_log("Login realizado com sucesso! Token de acesso recebido.".to_string()).await;
                    }
                    Err(e) => {
                        let mut st = self.state.write().await;
                        st.status = "Falha no Login".to_string();
                        st.add_log(format!("Falha ao autenticar: {}", e));
                        return Err(e.into());
                    }
                }
            }
        } else {
            self.add_log("Utilizando Bearer Token pré-configurado.".to_string()).await;
        }

        // 2. Carregar Me
        if let Ok(trainer) = self.client.get_me().await {
            let mut st = self.state.write().await;
            st.level = trainer.level;
            st.gold = trainer.gold;
            st.start_exp = trainer.exp;
            st.current_exp = trainer.exp;
            let log_msg = format!("Treinador conectado: {} (Nível {} | Ouro: {})", trainer.name, trainer.level, trainer.gold);
            st.add_log(log_msg);
        }

        // 3. Carregar Equipe e Criaturas
        if let Ok(creatures) = self.client.get_creatures("team").await {
            let mut st = self.state.write().await;
            if let Some(leader) = creatures.iter().find(|c| c.is_leader).or_else(|| creatures.first()) {
                st.hp = leader.hp;
                st.max_hp = leader.max_hp;
                st.leader_name = leader.name.clone();
                st.leader_level = leader.level;
                let log_msg = format!("Líder da equipe carregado: {} (Lv. {} | HP: {}/{})", leader.name, leader.level, leader.hp, leader.max_hp);
                st.add_log(log_msg);
            }
        }

        // 4. Carregar Zonas
        if let Ok(zones) = self.client.get_zones().await {
            let count = zones.len();
            let mut st = self.state.write().await;
            st.zones = zones;
            let log_msg = format!("Catálogo carregado: {} zonas de caça disponíveis.", count);
            st.add_log(log_msg);
        }

        {
            let mut st = self.state.write().await;
            st.status = "Pronto".to_string();
        }

        Ok(())
    }

    pub async fn run(&self) {
        // Loop de reconexão contínua
        let bot_clone = self.clone();
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(3);
            loop {
                let token = bot_clone.client.get_token().await;
                if token.is_empty() {
                    tokio::time::sleep(backoff).await;
                    continue;
                }

                let ws_url = format!("wss://pokepixel.nietore.com/api/v1/ws?token={}", urlencoding::encode(&token));
                bot_clone.add_log("Conectando ao canal em tempo real WebSocket...".to_string()).await;

                let mut req = match ws_url.into_client_request() {
                    Ok(r) => r,
                    Err(e) => {
                        bot_clone.add_log(format!("Erro ao criar requisição WS: {}", e)).await;
                        tokio::time::sleep(backoff).await;
                        continue;
                    }
                };

                req.headers_mut().insert(
                    "User-Agent",
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                        .parse()
                        .unwrap(),
                );
                req.headers_mut().insert("Origin", "https://pokepixel.nietore.com".parse().unwrap());

                match connect_async(req).await {
                    Ok((ws_stream, _)) => {
                        bot_clone.add_log("WebSocket conectado com sucesso! Sessão autoritativa iniciada.".to_string()).await;
                        backoff = Duration::from_secs(3);

                        let (mut write, mut read) = ws_stream.split();
                        let (tx, mut rx) = mpsc::channel::<String>(32);

                        *bot_clone.ws_sender.write().await = Some(tx);

                        let write_handle = tokio::spawn(async move {
                            // Envia resume inicial
                            let resume = serde_json::json!({
                                "type": "hunt.resume",
                                "data": { "last_seq": 0 }
                            });
                            let _ = write.send(Message::Text(resume.to_string())).await;

                            while let Some(msg) = rx.recv().await {
                                if let Err(e) = write.send(Message::Text(msg)).await {
                                    warn!("Falha ao enviar mensagem WS: {}", e);
                                    break;
                                }
                            }
                        });

                        while let Some(msg_res) = read.next().await {
                            match msg_res {
                                Ok(Message::Text(text)) => {
                                    bot_clone.handle_ws_message(&text).await;
                                }
                                Ok(Message::Close(_)) => {
                                    bot_clone.add_log("WebSocket fechado pelo servidor.".to_string()).await;
                                    break;
                                }
                                Err(e) => {
                                    bot_clone.add_log(format!("Erro no stream do WebSocket: {}", e)).await;
                                    break;
                                }
                                _ => {}
                            }
                        }

                        *bot_clone.ws_sender.write().await = None;
                        write_handle.abort();
                    }
                    Err(e) => {
                        bot_clone.add_log(format!("Falha na conexão do WebSocket: {}", e)).await;
                    }
                }

                // Reconexão e reautenticação
                {
                    let mut st = bot_clone.state.write().await;
                    st.status = "Reconectando...".to_string();
                    st.is_hunting = false;
                }

                bot_clone.add_log(format!("Tentando reautenticar em {}s...", backoff.as_secs())).await;
                tokio::time::sleep(backoff).await;

                if !bot_clone.config.credentials.email.is_empty() && !bot_clone.config.credentials.password.is_empty() {
                    match bot_clone.client.login(&bot_clone.config.credentials.email, &bot_clone.config.credentials.password).await {
                        Ok(_) => {
                            bot_clone.add_log("Reautenticação HTTP bem-sucedida!".to_string()).await;
                        }
                        Err(e) => {
                            bot_clone.add_log(format!("Reautenticação HTTP pendente ({}).", e)).await;
                            if backoff < Duration::from_secs(45) {
                                backoff += Duration::from_secs(5);
                            }
                        }
                    }
                }
            }
        });

        // Loop de Ping (a cada 20s)
        let bot_ping = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(20)).await;
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                let ping = serde_json::json!({
                    "type": "ping",
                    "data": {
                        "client_time": now_ms,
                        "foreground": true
                    }
                });
                let _ = bot_ping.send_ws_message(ping.to_string()).await;
            }
        });

        // Loop de Caça (a cada 5s)
        let bot_hunt = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if !bot_hunt.config.settings.auto_hunt {
                    continue;
                }

                let should_start = {
                    let st = bot_hunt.state.read().await;
                    !st.is_hunting && st.status != "Autenticando..." && st.status != "Falha no Login"
                };

                if should_start {
                    bot_hunt.trigger_hunt().await;
                }
            }
        });

        // Loop de Saúde (a cada 10s)
        let bot_heal = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if !bot_heal.config.settings.auto_heal {
                    continue;
                }

                let needs_heal = {
                    let st = bot_heal.state.read().await;
                    if st.max_hp > 0 {
                        let pct = (st.hp as f64 / st.max_hp as f64) * 100.0;
                        pct < bot_heal.config.settings.hp_threshold as f64
                    } else {
                        false
                    }
                };

                if needs_heal {
                    bot_heal.add_log("❤️ HP baixo detectado! Enviando ordem de cura da equipe...".to_string()).await;
                    if bot_heal.client.heal_team().await.is_ok() {
                        let mut st = bot_heal.state.write().await;
                        st.hp = st.max_hp;
                        st.add_log("✨ Equipe curada com sucesso!".to_string());
                    }
                }
            }
        });

        // Loop de Recompensas Diárias e Stamina (a cada 15 min)
        let bot_daily = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(900)).await;
                if bot_daily.config.settings.auto_daily {
                    bot_daily.add_log("🎁 Coletando recompensas diárias, missões e stamina...".to_string()).await;
                    let _ = bot_daily.client.claim_daily_login().await;
                    let _ = bot_daily.client.claim_all_quests().await;
                    let _ = bot_daily.client.claim_all_battlepass().await;
                    let _ = bot_daily.client.claim_all_species_goals().await;
                    let _ = bot_daily.client.claim_all_mail().await;
                    let _ = bot_daily.client.resolve_stamina().await;
                }

                if bot_daily.config.settings.auto_professions {
                    let _ = bot_daily.client.collect_mining_rewards(0).await;
                    let _ = bot_daily.client.collect_farm_rewards(0).await;
                }
            }
        });

        // Loop de Métricas de EXP/H
        let bot_metrics = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let mut st = bot_metrics.state.write().await;
                let elapsed = st.start_time.elapsed().as_secs_f64();
                if elapsed > 10.0 {
                    let gained_exp = (st.current_exp - st.start_exp).max(0) as f64;
                    st.exp_per_hour = (gained_exp / elapsed) * 3600.0;
                }
            }
        });
    }

    async fn trigger_hunt(&self) {
        let (zone_id, zone_name) = {
            let st = self.state.read().await;
            let target = &self.config.settings.hunt_target;

            let chosen = if target == "auto" || target.is_empty() {
                st.zones.iter().find(|z| z.recommended_level <= st.level).or_else(|| st.zones.first())
            } else {
                st.zones.iter().find(|z| &z.id == target).or_else(|| st.zones.first())
            };

            match chosen {
                Some(z) => (z.id.clone(), z.name.clone()),
                None => ("auto".to_string(), "Charmander".to_string()),
            }
        };

        self.add_log(format!("Preparando entrada na zona: {}...", zone_name)).await;

        match self.client.prepare_hunt(&zone_id).await {
            Ok(alloc) => {
                let enter_msg = serde_json::json!({
                    "type": "huntsim.enter",
                    "data": {
                        "map_id": alloc.map_id,
                        "zone_id": alloc.zone_id
                    }
                });

                let _ = self.send_ws_message(enter_msg.to_string()).await;

                let mut st = self.state.write().await;
                st.is_hunting = true;
                st.status = "hunting".to_string();
                st.current_area = zone_name.clone();
                st.current_map_id = alloc.map_id;
                st.current_zone_id = alloc.zone_id;
                let log_msg = format!("⚔️ Caçada ativa no mapa {} ({})!", alloc.map_id, zone_name);
                st.add_log(log_msg);
            }
            Err(e) => {
                self.add_log(format!("Erro ao preparar caçada em {}: {}", zone_name, e)).await;
            }
        }
    }

    async fn handle_ws_message(&self, text: &str) {
        let val: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return,
        };

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let data = val.get("data");

        match msg_type {
            "connection.ready" => {
                self.add_log("⚡ [WS] Servidor autorizou conexão (connection.ready).".to_string()).await;
                let mut st = self.state.write().await;
                st.status = "hunting".to_string();
                st.is_hunting = true;
            }
            "combat.started" => {
                let mut st = self.state.write().await;
                st.status = "hunting".to_string();
                st.is_hunting = true;
            }
            "loot.received" | "hunt.kill_reward" => {
                let gold = data.and_then(|d| d.get("gold")).and_then(|g| g.as_i64()).unwrap_or(0);
                let exp = data.and_then(|d| d.get("exp")).and_then(|e| e.as_i64()).unwrap_or(0);

                let mut st = self.state.write().await;
                st.kills += 1;
                st.gold += gold;
                st.current_exp += exp;
                st.status = "hunting".to_string();
            }
            "level.up" | "trainer.level_up" => {
                let lvl = data.and_then(|d| d.get("level")).and_then(|l| l.as_u64()).unwrap_or(0) as u32;
                let max_hp = data.and_then(|d| d.get("max_hp")).and_then(|h| h.as_u64()).unwrap_or(0) as u32;

                let mut st = self.state.write().await;
                if lvl > 0 {
                    st.level = lvl;
                }
                if max_hp > 0 {
                    st.max_hp = max_hp;
                    st.hp = max_hp;
                }
                let curr_lvl = st.level;
                st.add_log(format!("🎉 PARABÉNS! SUBIU PARA O NÍVEL {}!", curr_lvl));
            }
            "combat.player_defeated" => {
                self.add_log("⚠️ Pokémon líder foi derrotado em combate! Ativando protocolo de recuperação...".to_string()).await;
                let _ = self.client.heal_team().await;
            }
            "capture.success" => {
                let name = data.and_then(|d| d.get("name")).and_then(|n| n.as_str()).unwrap_or("Pokémon");
                let is_shiny = data.and_then(|d| d.get("is_shiny")).and_then(|s| s.as_bool()).unwrap_or(false);
                if is_shiny {
                    self.add_log(format!("🌟 CAPTURA SHINY INCRÍVEL: {}!", name)).await;
                } else {
                    self.add_log(format!("🎯 Pokémon capturado com sucesso: {}", name)).await;
                }
            }
            "hunt.stopped" => {
                let mut st = self.state.write().await;
                st.is_hunting = false;
            }
            _ => {}
        }
    }

    pub async fn to_dto(&self) -> AccountStatusDto {
        let st = self.state.read().await;
        let elapsed_sec = st.start_time.elapsed().as_secs();

        AccountStatusDto {
            id: self.config.id.clone(),
            name: if self.config.settings.stealth_mode {
                format!("Treinador #{}", self.config.id)
            } else {
                self.config.name.clone()
            },
            level: st.level,
            vocation: "Treinador".to_string(),
            hp: st.hp,
            max_hp: st.max_hp,
            mana: st.mana,
            max_mana: st.max_mana,
            gold: st.gold,
            kills: st.kills,
            exp_per_hour: st.exp_per_hour,
            status: st.status.clone(),
            current_area: st.current_area.clone(),
            duration_sec: elapsed_sec,
            logs: st.logs.iter().cloned().collect(),
            updated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            extra: ExtraStatusDto {
                bestiary_focus: self.config.settings.bestiary_focus,
                circuit_state: st.circuit_state.clone(),
                cooldown_sec: 0,
                leader_level: st.leader_level,
                leader_name: st.leader_name.clone(),
            },
        }
    }
}
