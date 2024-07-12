use crate::{fetcher::FetchStatus, ApiRegion, Player, Result, State};
use cached::proc_macro::io_cached;
use chrono::{DateTime, Utc};
use log::info;
use serde_json::Value;

pub mod json;

#[io_cached(
    disk = true,
    disk_dir = "cache",
    key = "Player",
    convert = r#"{ player.clone().normalized() }"#,
    map_error = r##"|e| e"##
)]
pub async fn get_puuid_raw(state: &State, player: &Player) -> Result<Value> {
    let region = ApiRegion::Europe;
    state
        .client
        .get::<Value>(
            region,
            "/riot/account/v1/accounts/by-riot-id",
            [player.game_name.as_str(), player.tag_line.as_str()],
            player,
        )
        .await
}

pub async fn get_puuid(state: &State, player: &Player) -> Result<String> {
    let json = get_puuid_raw(state, player).await?;
    let puuid = json["puuid"].as_str().ok_or("No PUUID")?;
    Ok(puuid.to_string())
}

pub async fn get_puuid_and_canonical_name(state: &State, player: &mut Player) -> Result<String> {
    let json = get_puuid_raw(state, player).await?;
    let puuid = json["puuid"].as_str().ok_or("No PUUID")?;
    let game_name = json["gameName"].as_str().ok_or("No game name")?;
    let tag_line = json["tagLine"].as_str().ok_or("No tag line")?;
    player.game_name = game_name.to_string();
    player.tag_line = tag_line.to_string();
    Ok(puuid.to_string())
}

#[io_cached(
    disk = true,
    disk_dir = "cache",
    key = "String",
    convert = r#"{ format!("{region}#{match_id}") }"#,
    map_error = r##"|e| e"##
)]
pub async fn get_match(state: &State, region: ApiRegion, match_id: &str, player: &Player) -> Result<json::Match> {
    state
        .client
        .get::<json::Match>(region, "/lol/match/v5/matches", [match_id], player)
        .await
}

#[io_cached(
    disk = true,
    disk_dir = "cache",
    key = "String",
    convert = r#"{ format!("{region}#{match_id}") }"#,
    map_error = r##"|e| e"##
)]
pub async fn get_match_timeline(
    state: &State,
    region: ApiRegion,
    match_id: &str,
    player: &Player,
) -> Result<json::Timeline> {
    state
        .client
        .get::<json::Timeline>(region, "/lol/match/v5/matches", [match_id, "timeline"], player)
        .await
}

pub async fn get_match_history(
    state: &State,
    region: ApiRegion,
    puuid: &str,
    end: Option<DateTime<Utc>>,
    player: &Player,
) -> Result<Vec<String>> {
    let mut query_params = vec![("count", "40"), ("queue", "420")];
    let end_string = end.map(|end| format!("{}", end.timestamp()));
    if let Some(ref end_string) = end_string {
        query_params.push(("endTime", end_string));
    }
    state
        .client
        .get_with_query::<Vec<String>>(
            region,
            "/lol/match/v5/matches/by-puuid",
            [puuid, "ids"],
            query_params,
            player,
        )
        .await
}

pub async fn update_match_history(
    state: &State,
    player: &Player,
    start: DateTime<Utc>,
) -> Result<()> {
    info!("Updating match history for {player}");
    let puuid = get_puuid(state, player).await?;
    let region = player.region.into();
    let mut match_ids = vec![];
    let mut earliest_match = None;
    while earliest_match.map_or(true, |earliest| earliest > start) {
        match_ids.extend(get_match_history(state, region, &puuid, earliest_match, player).await?);
        earliest_match = match match_ids.last() {
            Some(match_id) => {
                let match_info = get_match(state, region, match_id, player).await?;
                Some(match_info.info.game_start_timestamp)
            }
            None => return Err("No matches found".into()),
        };
    }
    let mut matches = state.matches_per_puuid.entry(puuid).or_default();
    for (index, match_id) in match_ids.iter().enumerate() {
        if !matches.contains_key(match_id) {
            matches.insert(
                match_id.to_string(),
                get_match(state, region, match_id, player).await?,
            );
        }
        if !state.timeline_per_match.contains_key(match_id) {
            state.timeline_per_match.insert(
                match_id.to_string(),
                get_match_timeline(state, region, match_id, player).await?,
            );
        }
        if let Some(mut broadcaster) = state.fetch_status_per_player.get_mut(player) {
            broadcaster
                .broadcast(FetchStatus::Fetching {
                    percent_done: u8::try_from((index + 1) * 100 / match_ids.len()).unwrap(),
                })
                .await;
        }
    }
    Ok(())
}
