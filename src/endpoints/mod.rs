use crate::{internal_server_error, LeagueRegion};
use actix_files::NamedFile;
use actix_web::{routes, Responder, Result as ActixResult};
use askama_actix::Template;
use strum::IntoEnumIterator;

pub mod compare;
pub mod fetch;
pub mod stats;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexDisplayData {
    regions: Vec<String>,
}

pub async fn index() -> ActixResult<impl Responder> {
    Ok(IndexDisplayData {
        regions: LeagueRegion::iter()
            .map(|region| region.to_string())
            .collect(),
    }
    .render()
    .map_err(internal_server_error)?
    .customize()
    .insert_header(("content-type", "text/html")))
}

#[routes]
#[get("/riot.txt")]
#[get("//riot.txt")]
pub async fn riot_txt() -> ActixResult<NamedFile> {
    NamedFile::open_async("riot.txt")
        .await
        .map_err(internal_server_error)
}
