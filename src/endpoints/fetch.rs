use crate::{
    fetcher::check_or_start_fetching, internal_server_error, Player, State,
};
use actix_web::{routes, web, Either, HttpResponse, Responder, Result as ActixResult};
use askama_actix::Template;
use serde::Deserialize;

#[routes]
#[get("/fetch-events/{region}/{game_name}/{tag_line}")]
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

#[derive(Deserialize)]
struct Params {
    from: Option<String>,
}

#[routes]
#[get("/fetch/{region}/{game_name}/{tag_line}")]
pub async fn page(state: State, path: web::Path<Player>, query: web::Query<Params>) -> ActixResult<impl Responder> {
    let player = path.into_inner().normalized();
    let _ = check_or_start_fetching(state.clone(), &player, query.from.as_deref())
        .await
        .map_err(internal_server_error)?;
    
    Ok(DisplayData { player }
        .render()
        .map_err(internal_server_error)?
        .customize()
        .insert_header(("content-type", "text/html")))
}
