use crate::{
    fetcher::check_or_start_fetching, internal_server_error, Player, PlayerRoleChamp, State,
};
use actix_web::{routes, web, Either, HttpResponse, Responder, Result as ActixResult};
use askama_actix::Template;

#[routes]
#[get("/fetch-events/{region}/{game_name}/{tag_line}")]
#[get("/fetch-events/{region}/{game_name}/{tag_line}/{role}/{champion}")]
pub async fn events(state: State, path: web::Path<Player>) -> ActixResult<impl Responder> {
    let player = path.into_inner().normalized();
    let broadcaster = state.fetch_status_per_player.get_mut(&player);
    if let Some(mut broadcaster) = broadcaster {
        Ok(Either::Right(broadcaster.add_client().await))
    } else {
        Ok(Either::Left(
            HttpResponse::NotFound().body("No such fetch in progress"),
        ))
    }
}

#[derive(Template)]
#[template(path = "fetch.html")]
struct DisplayData {
    player: Player,
}

#[routes]
#[get("/fetch/{region}/{game_name}/{tag_line}")]
#[get("/fetch/{region}/{game_name}/{tag_line}/{role}/{champion}")]
pub async fn page(state: State, path: web::Path<PlayerRoleChamp>) -> ActixResult<impl Responder> {
    let (player, role, champion) = path.into_inner().into();
    let _ = check_or_start_fetching(state.clone(), &player, role, champion.as_deref())
        .await
        .map_err(internal_server_error)?;
    Ok(DisplayData { player }
        .render()
        .map_err(internal_server_error)?
        .customize()
        .insert_header(("content-type", "text/html")))
}
