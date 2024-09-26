use crate::{
    calculations::GroupStats,
    fetcher::{check_or_start_fetching, RedirectOrContinue},
    internal_server_error,
    riot_api::json::Role,
    LeagueRegion, Player, State, CHAMP_NAMES,
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

struct ChampionStats {
    name: String,
    id: String,
    total_games: u32,
    stats: [Option<GroupStats>; 2],
}

type PerRoleChampStats = Vec<(Role, Vec<ChampionStats>)>;
type PerGroupChampStats = HashMap<String, PerRoleChampStats>;

struct ChampionStatsIntermediate {
    id: String,
    player1: Option<GroupStats>,
    player2: Option<GroupStats>,
}

type ChampStatsMap = HashMap<String, ChampionStatsIntermediate>;
type RoleStatsMap = HashMap<Role, ChampStatsMap>;
type GroupStatsMap = HashMap<String, RoleStatsMap>;

#[derive(Template)]
#[template(path = "compare2.html", escape = "none")]
struct DisplayData {
    players: [Player; 2],
    role: Option<Role>,
    champion_name: Option<String>,
    champion_id: Option<String>,
    data: HashMap<(Player, String), GroupStats>,
    group_titles_and_ids: Vec<(String, String)>,
    per_group_per_role_per_champ: PerGroupChampStats,
}

impl DisplayData {
    // We have to pass by ref, because that's what Askama generates
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn get_group<'a>(&'a self, player: &Player, name: &&String) -> Option<&'a GroupStats> {
        self.data.get(&(player.clone(), (*name).to_string()))
    }
    // We have to pass by ref, because that's what Askama generates
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn get_per_role<'a>(&'a self, group_id: &&String) -> Option<&'a PerRoleChampStats> {
        self.per_group_per_role_per_champ.get(*group_id)
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
    group_titles_and_ids: &mut HashSet<(String, String)>,
) -> ActixResult<HashMap<(Player, String), GroupStats>> {
    let groups = crate::calculations::calc_stats(state.clone(), player, role, champion.as_deref())
        .await
        .map_err(internal_server_error)?;
    for group in &groups {
        group_titles_and_ids.insert((group.title.clone(), group.id.clone()));
    }
    Ok(groups
        .into_iter()
        .map(|group| ((player.clone(), group.id.clone()), group))
        .collect::<HashMap<_, _>>())
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
    let mut group_titles_and_ids = HashSet::new();
    let mut p1d = do_player(
        state.clone(),
        &mut p1,
        role,
        &champion,
        &mut group_titles_and_ids,
    )
    .await
    .map_err(internal_server_error)?;
    let p2d = do_player(
        state.clone(),
        &mut p2,
        role,
        &champion,
        &mut group_titles_and_ids,
    )
    .await
    .map_err(internal_server_error)?;
    let mut per_group_per_role_per_champ: GroupStatsMap = HashMap::new();

    compare_players(&mut p1d, &p2d, &mut per_group_per_role_per_champ);

    let mut group_titles_and_ids = group_titles_and_ids
        .into_iter()
        .sorted()
        .collect::<Vec<_>>();
    // Pop Total off the front and put it at the end
    let total = group_titles_and_ids.remove(0);
    group_titles_and_ids.push(total);

    let champion_name = champion
        .as_ref()
        .map(|c| (*CHAMP_NAMES.get(c).unwrap()).to_string());

    let mut data = p1d;
    data.extend(p2d);

    Ok(Either::Right(
        DisplayData {
            players: [p1, p2],
            role,
            champion_name,
            champion_id: champion,
            data,
            group_titles_and_ids,
            per_group_per_role_per_champ: get_per_group_per_role_per_champ(
                per_group_per_role_per_champ,
            ),
        }
        .customize()
        .insert_header(("content-type", "text/html")),
    ))
}

fn get_per_group_per_role_per_champ(
    per_group_per_role_per_champ: GroupStatsMap,
) -> PerGroupChampStats {
    per_group_per_role_per_champ
        .into_iter()
        .map(|(group_id, per_role_per_champ)| {
            (
                group_id,
                per_role_per_champ
                    .into_iter()
                    .map(|(role, champs)| {
                        let champs = champs
                            .into_iter()
                            .map(
                                |(
                                    name,
                                    ChampionStatsIntermediate {
                                        id,
                                        player1,
                                        player2,
                                    },
                                )| {
                                    let total_games =
                                        player1.as_ref().map_or(0, |s| s.games_played)
                                            + player2.as_ref().map_or(0, |s| s.games_played);
                                    ChampionStats {
                                        name,
                                        id,
                                        total_games,
                                        stats: [player1, player2],
                                    }
                                },
                            )
                            // Sort by games played, descending
                            .sorted_by_key(|cs| {
                                -i64::from(
                                    cs.stats
                                        .iter()
                                        .map(|s| s.as_ref().map_or(0, |s| s.games_played))
                                        .sum::<u32>(),
                                )
                            })
                            .collect::<Vec<_>>();
                        (role, champs)
                    })
                    // Sort by games played
                    .sorted_by_cached_key(|per_role| {
                        -i64::from(
                            per_role
                                .1
                                .iter()
                                .flat_map(|cs| {
                                    cs.stats
                                        .iter()
                                        .map(|s| s.as_ref().map_or(0, |s| s.games_played))
                                })
                                .sum::<u32>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<HashMap<_, _>>()
}

fn compare_players(
    p1d: &mut HashMap<(Player, String), GroupStats>,
    p2d: &HashMap<(Player, String), GroupStats>,
    per_group_per_role_per_champ: &mut GroupStatsMap,
) {
    let p2 = p2d.keys().next().unwrap().0.clone();
    for ((p1, group_name), p1_group) in p1d {
        if let Some(p2_group) = p2d.get(&(p2.clone(), group_name.clone())) {
            debug!("Comparing {p1} and {p2} in {group_name}");
            p1_group.compare_to(p2_group);
            let p1_per_role_per_champ = p1_group
                .per_role_per_champ
                .iter()
                .cloned()
                .collect::<HashMap<_, _>>();
            let p2_per_role_per_champ = p2_group
                .per_role_per_champ
                .iter()
                .cloned()
                .collect::<HashMap<_, _>>();
            let all_roles = p1_per_role_per_champ
                .keys()
                .chain(p2_per_role_per_champ.keys())
                .copied()
                .sorted()
                .dedup()
                .collect::<Vec<_>>();
            let group_entry = per_group_per_role_per_champ
                .entry(group_name.clone())
                .or_default();
            for role in all_roles {
                let role_entry = group_entry.entry(role).or_default();
                let p1_champ_stats = p1_per_role_per_champ.get(&role);
                let p2_champ_stats = p2_per_role_per_champ.get(&role);
                let p1_champ_stats = p1_champ_stats.map(|v| {
                    v.iter()
                        .map(|(name, id, stats)| (id, (name, stats)))
                        .collect::<HashMap<_, _>>()
                });
                let p2_champ_stats = p2_champ_stats.map(|v| {
                    v.iter()
                        .map(|(name, id, stats)| (id, (name, stats)))
                        .collect::<HashMap<_, _>>()
                });
                let all_champs = p1_champ_stats
                    .iter()
                    .chain(p2_champ_stats.iter())
                    .flatten()
                    .map(|(id, (name, _stats))| (*name, *id))
                    .sorted()
                    .dedup()
                    .collect::<Vec<_>>();
                for (champ_name, champ_id) in all_champs {
                    let mut p1_stats = None;
                    let mut p2_stats = None;
                    if let Some(ref p1_champ_stats) = p1_champ_stats {
                        p1_stats = p1_champ_stats
                            .get(&champ_id)
                            .map(|(_name, stats)| (*stats).clone());
                    }
                    if let Some(ref p2_champ_stats) = p2_champ_stats {
                        p2_stats = p2_champ_stats
                            .get(&champ_id)
                            .map(|(_name, stats)| (*stats).clone());
                    }
                    role_entry.insert(
                        champ_name.to_string(),
                        ChampionStatsIntermediate {
                            id: champ_id.to_string(),
                            player1: p1_stats,
                            player2: p2_stats,
                        },
                    );
                }
            }
        }
    }
}
