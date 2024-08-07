use crate::{
    calculations::WeekStats,
    fetcher::{check_or_start_fetching, RedirectOrContinue},
    internal_server_error,
    riot_api::json::Role,
    LeagueRegion, Player, State,
};
use actix_web::{routes, web, Either, HttpRequest, Responder, Result as ActixResult};
use askama_actix::Template;
use itertools::Itertools;
use log::debug;
use serde::Deserialize;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

#[derive(Template)]
#[template(path = "compare2.html", escape = "none")]
struct DisplayData {
    players: [Player; 2],
    role: Option<Role>,
    champion: Option<String>,
    data: HashMap<(Player, i64), WeekStats>,
    week_numbers: Vec<i64>,
}

impl DisplayData {
    // We have to pass by ref, because that's what Askama generates
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn get_week<'a>(&'a self, player: &Player, week: &&i64) -> Option<&'a WeekStats> {
        self.data.get(&(player.clone(), **week))
    }
}

#[derive(Deserialize)]
struct Params2 {
    region1: LeagueRegion,
    game_name1: String,
    tag_line1: String,
    region2: LeagueRegion,
    game_name2: String,
    tag_line2: String,
    role: Option<Role>,
    champion: Option<String>,
}

impl Params2 {
    fn into(self) -> (Player, Player, Option<Role>, Option<String>) {
        (
            Player {
                region: self.region1,
                game_name: self.game_name1,
                tag_line: self.tag_line1,
            }
            .normalized(),
            Player {
                region: self.region2,
                game_name: self.game_name2,
                tag_line: self.tag_line2,
            }
            .normalized(),
            self.role,
            self.champion,
        )
    }
}

async fn do_player(
    state: State,
    player: &mut Player,
    role: Option<Role>,
    champion: &Option<String>,
    week_numbers: &mut HashSet<i64>,
) -> ActixResult<HashMap<(Player, i64), WeekStats>> {
    let weeks = crate::calculations::calc_stats(state.clone(), player, role, champion.as_deref())
        .await
        .map_err(internal_server_error)?;
    for week in &weeks {
        week_numbers.insert(week.number);
    }
    let pd = weeks
        .into_iter()
        .map(|week| ((player.clone(), week.number), week))
        .collect::<HashMap<_, _>>();
    Ok(pd)
}
#[routes]
#[get("/compare/{region1}/{game_name1}/{tag_line1}/vs/{region2}/{game_name2}/{tag_line2}")]
#[get("/compare/{region1}/{game_name1}/{tag_line1}/vs/{region2}/{game_name2}/{tag_line2}/{role}")]
#[get("/compare/{region1}/{game_name1}/{tag_line1}/vs/{region2}/{game_name2}/{tag_line2}/{role}/{champion}")]
pub async fn page2(
    state: State,
    request: HttpRequest,
    path: web::Path<Params2>,
) -> ActixResult<impl Responder> {
    let (mut p1, mut p2, role, champion) = path.into_inner().into();
    let mut week_numbers = HashSet::new();
    for player in [&p1, &p2] {
        debug!("Getting stats for {player} in {role:?} as {champion:?}");
        if let RedirectOrContinue::Redirect(redirect) =
            check_or_start_fetching(state.clone(), player, Some(request.path()))
                .await
                .map_err(internal_server_error)?
        {
            return Ok(Either::Left(redirect));
        }
    }
    let mut p1d = do_player(state.clone(), &mut p1, role, &champion, &mut week_numbers)
        .await
        .map_err(internal_server_error)?;
    let p2d = do_player(state.clone(), &mut p2, role, &champion, &mut week_numbers)
        .await
        .map_err(internal_server_error)?;
    for ((p1, week_number), p1_week) in &mut p1d {
        if let Some(p2_week) = p2d.get(&(p2.clone(), *week_number)) {
            debug!("Comparing {p1} and {p2} in week {week_number}");
            p1_week.compare_to(p2_week);
        }
    }
    let mut data = p1d;
    data.extend(p2d);
    Ok(Either::Right(
        DisplayData {
            players: [p1, p2],
            role,
            champion,
            data,
            week_numbers: week_numbers.into_iter().sorted().collect(),
        }
        .customize()
        .insert_header(("content-type", "text/html")),
    ))
}
