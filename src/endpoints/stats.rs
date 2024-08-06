use crate::{
    calculations::WeekStats, fetcher::{check_or_start_fetching, RedirectOrContinue}, internal_server_error, riot_api::json::Role, Player, PlayerRoleChamp, State, CHAMP_NAMES
};
use actix_web::{routes, web, Either, HttpRequest, Responder, Result as ActixResult};
use askama_actix::Template;
use log::debug;
use std::cmp::Ordering;

#[derive(Template)]
#[template(path = "stats.html", escape = "none")]
struct DisplayData {
    player: Player,
    role: Option<Role>,
    champion: Option<String>,
    weeks: Vec<WeekStats>,
}

#[routes]
#[get("/stats/{region}/{game_name}/{tag_line}")]
#[get("/stats/{region}/{game_name}/{tag_line}/{role}")]
#[get("/stats/{region}/{game_name}/{tag_line}/{role}/{champion}")]
pub async fn page(state: State, request: HttpRequest, path: web::Path<PlayerRoleChamp>) -> ActixResult<impl Responder> {
    let (mut player, role, champion) = path.into_inner().into();
    debug!("Getting stats for {player} in {role:?} as {champion:?}");
    if let RedirectOrContinue::Redirect(redirect) =
        check_or_start_fetching(state.clone(), &player, Some(request.path()))
            .await
            .map_err(internal_server_error)?
    {
        return Ok(Either::Left(redirect));
    }
    let mut weeks =
        crate::calculations::calc_stats(state, &mut player, role, champion.as_deref())
            .await
            .map_err(internal_server_error)?;
    let mut previous_week = None;
    for current_week in &mut weeks {
        if let Some(previous_week) = previous_week {
            current_week.compare_to(&previous_week);
        }
        previous_week = Some(current_week.clone());
    }
    let champion = champion.map(|c| (*CHAMP_NAMES.get(&c).unwrap()).to_string());
    Ok(Either::Right(
        DisplayData {
            player,
            role,
            champion,
            weeks,
        }
        .customize()
        .insert_header(("content-type", "text/html")),
    ))
}
