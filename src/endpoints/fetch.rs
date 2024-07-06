use crate::{internal_server_error, Player, State};
use actix_web::{get, web, Either, HttpResponse, Responder, Result as ActixResult};
use askama_actix::Template;

#[get("/fetch/{region}/{game_name}/{tag_line}/events")]
pub async fn page(data: State, path: web::Path<Player>) -> ActixResult<impl Responder> {
    let player = path.into_inner().normalized();
    let broadcaster = data.fetch_status_per_player.get_mut(&player);
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

#[get("/fetch/{region}/{game_name}/{tag_line}")]
pub async fn events(path: web::Path<Player>) -> ActixResult<impl Responder> {
    let player = path.into_inner().normalized();
    Ok(DisplayData { player }
        .render()
        .map_err(internal_server_error)?
        .customize()
        .insert_header(("content-type", "text/html")))
}
