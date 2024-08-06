use actix_web::web::Redirect;
use actix_web_lab::{
    sse::{self, Sse},
    util::InfallibleStream,
};
use chrono::Utc;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    riot_api::{json::Role, update_match_history},
    Player, Result, State,
};

#[derive(Debug, Clone, Serialize)]
pub enum FetchStatus {
    Starting,
    Fetching { percent_done: u8 },
    Waiting { seconds_left: u64 },
    Done,
    Error(String),
}

// This should be infallible, so unwrap()
impl From<FetchStatus> for sse::Event {
    fn from(status: FetchStatus) -> Self {
        Self::Data(sse::Data::new_json(status).unwrap())
    }
}

impl From<&FetchStatus> for sse::Event {
    fn from(status: &FetchStatus) -> Self {
        Self::Data(sse::Data::new_json(status).unwrap())
    }
}

#[derive(Debug)]
pub struct StatusBroadcaster {
    last_status: FetchStatus,
    clients: Vec<mpsc::Sender<sse::Event>>,
}

impl StatusBroadcaster {
    fn new() -> Self {
        Self {
            last_status: FetchStatus::Starting,
            clients: Vec::new(),
        }
    }

    async fn send(&mut self, event: sse::Event) {
        let clients = self.clients.clone();
        let mut ok_clients = Vec::new();
        for client in clients {
            if client.send(event.clone()).await.is_ok() {
                ok_clients.push(client);
            }
        }
        self.clients = ok_clients;
    }

    pub async fn broadcast(&mut self, status: FetchStatus) {
        let event = sse::Event::from(&status);
        self.last_status = status;
        self.send(event).await;
    }

    pub async fn keepalive(&mut self) {
        let event = sse::Event::Comment("keepalive".into());
        self.send(event).await;
    }

    pub async fn add_client(&mut self) -> Sse<InfallibleStream<ReceiverStream<sse::Event>>> {
        let (tx, rx) = mpsc::channel::<sse::Event>(10);
        // Since we just created, shouldn't fail
        tx.send(sse::Event::from(&self.last_status)).await.unwrap();
        self.clients.push(tx);
        Sse::from_infallible_receiver(rx)
    }
}

pub enum RedirectOrContinue {
    Redirect(Redirect),
    Continue,
}

pub async fn check_or_start_fetching(
    state: State,
    player: &Player,
    role: Option<Role>,
    champion: Option<&str>,
) -> Result<RedirectOrContinue> {
    let mut broadcaster_ref = state
        .fetch_status_per_player
        .entry(player.clone())
        .or_insert_with(StatusBroadcaster::new);
    let broadcaster = &mut *broadcaster_ref;
    if matches!(
        broadcaster.last_status,
        FetchStatus::Starting | FetchStatus::Error(_)
    ) {
        let player_clone = player.clone();
        let state = state.clone();
        let from = Utc::now() - chrono::Duration::days(28);
        tokio::spawn(async move {
            let status = match update_match_history(&state, &player_clone, from).await {
                Ok(()) => FetchStatus::Done,
                Err(e) => FetchStatus::Error(e.to_string()),
            };
            state
                .fetch_status_per_player
                .get_mut(&player_clone)
                .unwrap()
                .broadcast(status)
                .await;
        });
        broadcaster.last_status = FetchStatus::Fetching { percent_done: 0 };
    }
    match broadcaster.last_status {
        FetchStatus::Done => Ok(RedirectOrContinue::Continue),
        FetchStatus::Starting => unreachable!("Starting status should have been changed"),
        _ => {
            let mut url = format!(
                "/fetch/{region}/{game_name}/{tag_line}",
                region = player.region,
                game_name = player.game_name,
                tag_line = player.tag_line
            );
            if let Some(role) = role {
                url.push('/');
                url.push_str(&role.lowercase());
                if let Some(champion) = champion {
                    url.push('/');
                    url.push_str(champion);
                }    
            }
            Ok(RedirectOrContinue::Redirect(Redirect::to(url)))
        }
    }
}
