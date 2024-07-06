use crate::{internal_server_error, LeagueRegion};
use actix_web::{Responder, Result as ActixResult};
use askama_actix::Template;
use strum::IntoEnumIterator;

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
