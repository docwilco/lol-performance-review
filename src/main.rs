#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use actix_files::Files;
use actix_web::{error::ErrorInternalServerError, web, App, HttpServer};
use dashmap::DashMap;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    fmt::{self, Display, Formatter}, sync::Arc,
};

mod fetcher;
use fetcher::StatusBroadcaster;
mod endpoints;
mod ratelimiter;
mod riot_api;
use riot_api::json;

type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
type Result<T> = std::result::Result<T, Error>;

#[allow(clippy::upper_case_acronyms)]
#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Hash, strum::Display, strum::EnumIter)]
enum LeagueRegion {
    BR,
    EUNE,
    EUW,
    JP,
    KR,
    LAN,
    LAS,
    ME1,
    NA,
    OCE,
    PH2,
    SG2,
    TH2,
    TR,
    TW2,
    RU,
    VN2,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Copy, Clone, Debug, strum::Display)]
enum ApiRegion {
    Americas,
    Asia,
    Europe,
    SEA,
}

impl ApiRegion {
    fn hostname(self) -> &'static str {
        match self {
            ApiRegion::Americas => "americas.api.riotgames.com",
            ApiRegion::Asia => "asia.api.riotgames.com",
            ApiRegion::Europe => "europe.api.riotgames.com",
            ApiRegion::SEA => "sea.api.riotgames.com",
        }
    }
}

impl From<LeagueRegion> for ApiRegion {
    fn from(region: LeagueRegion) -> Self {
        match region {
            LeagueRegion::NA | LeagueRegion::BR | LeagueRegion::LAN | LeagueRegion::LAS => {
                ApiRegion::Americas
            }
            LeagueRegion::KR | LeagueRegion::JP => ApiRegion::Asia,
            LeagueRegion::EUNE
            | LeagueRegion::EUW
            | LeagueRegion::ME1
            | LeagueRegion::TR
            | LeagueRegion::RU => ApiRegion::Europe,
            LeagueRegion::OCE
            | LeagueRegion::PH2
            | LeagueRegion::SG2
            | LeagueRegion::TH2
            | LeagueRegion::TW2
            | LeagueRegion::VN2 => ApiRegion::SEA,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
struct Player {
    region: LeagueRegion,
    game_name: String,
    tag_line: String,
}

impl Display for Player {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}#{} ({})", self.game_name, self.tag_line, self.region)
    }
}

impl Player {
    fn normalize(&mut self) {
        self.game_name = self.game_name.to_lowercase();
        self.tag_line = self.tag_line.to_uppercase();
    }
    fn normalized(mut self) -> Self {
        self.normalize();
        self
    }
}

type FetchStatusPerPlayer = Arc<DashMap<Player, StatusBroadcaster>>;
struct InnerState {
    client: ratelimiter::ApiClient,
    matches_per_puuid: DashMap<String, HashMap<String, json::Match>>,
    timeline_per_match: DashMap<String, json::Timeline>,
    fetch_status_per_player: FetchStatusPerPlayer,
}

type State = web::Data<InnerState>;

fn internal_server_error<T>(err: T) -> actix_web::Error
where
    T: fmt::Debug + fmt::Display + 'static,
{
    log::error!("{:?}", err);
    ErrorInternalServerError(err)
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv()?;
    env_logger::init();

    let api_key = env::var("RIOT_API_KEY")?;
    let fetch_status_per_player = Arc::new(DashMap::new());
    let client = ratelimiter::ApiClient::new(&api_key, fetch_status_per_player.clone())?;
    let state = InnerState {
        client,
        matches_per_puuid: DashMap::new(),
        timeline_per_match: DashMap::new(),
        fetch_status_per_player,
    };
    let data = web::Data::new(state);
    let state = data.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            for mut broadcaster in state.fetch_status_per_player.iter_mut() {
                broadcaster.keepalive().await;
            }
        }
    });

    let server = HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .service(Files::new("/static", "static"))
            .route("/", web::get().to(endpoints::index))
            .service(endpoints::stats::page)
            .service(endpoints::fetch::page)
            .service(endpoints::fetch::events)
    });
    server.bind(("127.0.0.1", 8080))?.run().await?;

    Ok(())
}
