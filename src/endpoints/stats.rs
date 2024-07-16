use crate::{
    fetcher::{check_or_start_fetching, RedirectOrContinue},
    internal_server_error,
    riot_api::{
        get_puuid_and_canonical_name,
        json::{self, Role},
        update_match_history,
    },
    Player, Result, State,
};
use actix_web::{get, web, Either, Responder, Result as ActixResult};
use askama_actix::Template;
use chrono::{TimeDelta, Utc};
use itertools::{Itertools, Position};
use log::debug;
use ordered_float::OrderedFloat;
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

const MINUTES_AT: [u32; 4] = [5, 10, 15, 20];

fn median_f64<'a>(values: impl IntoIterator<Item = &'a f64>) -> f64 {
    let mut values = values
        .into_iter()
        .copied()
        .map(OrderedFloat::from)
        .collect::<Vec<_>>();
    values.sort_unstable();
    let mid = values.len() / 2;
    let median = if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[values.len() / 2]
    };
    median.into_inner()
}

fn median<T>(values: impl IntoIterator<Item = T>) -> f64
where
    T: Copy + Into<f64> + Ord,
{
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1].into() + values[mid].into()) / 2.0
    } else {
        values[mid].into()
    }
}

fn _average_f64<'a>(values: impl IntoIterator<Item = &'a f64>) -> f64 {
    let (sum, count) = values
        .into_iter()
        .fold((0.0, 0), |(sum, count), value| (sum + *value, count + 1));
    sum / count as f64
}

fn average<T>(values: impl IntoIterator<Item = T>) -> f64
where
    T: Copy + Into<f64>,
{
    let mut sum = 0.0;
    let mut count = 0;
    for value in values {
        sum += Into::<f64>::into(value);
        count += 1;
    }
    sum / count as f64
}

fn frame_stats_at(
    frames: &[json::Frame],
    player: usize,
    opponent: usize,
    timestamp: TimeDelta,
) -> Option<StatsAtMinuteGathering> {
    let frame = frames.iter().find(|f| f.timestamp >= timestamp)?;
    let cpm = frame.participant_frames.get(&player)?.minions_killed as f64
        / timestamp.num_minutes() as f64;
    let gold_diff = (frame.participant_frames.get(&player)?.total_gold)
        - (frame.participant_frames.get(&opponent)?.total_gold);
    let cs_diff = (frame.participant_frames.get(&player)?.minions_killed)
        - (frame.participant_frames.get(&opponent)?.minions_killed);
    let xp_diff =
        (frame.participant_frames.get(&player)?.xp) - (frame.participant_frames.get(&opponent)?.xp);
    Some(StatsAtMinuteGathering {
        cs_per_minute: cpm,
        gold_diff,
        cs_diff,
        xp_diff,
    })
}

fn timeline_get_player_id(timeline: &json::Timeline, puuid: &str) -> usize {
    timeline
        .info
        .participants
        .iter()
        .find(|p| p.puuid == puuid)
        .unwrap()
        .participant_id
}

fn get_player<'a>(match_info: &'a json::Match, puuid: &'a str) -> &'a json::Participant {
    match_info
        .info
        .participants
        .iter()
        .find(|p| p.puuid == puuid)
        .unwrap()
}

fn get_opponent<'a>(
    match_info: &'a json::Match,
    player: &'a json::Participant,
) -> &'a json::Participant {
    match_info
        .info
        .participants
        .iter()
        .find(|p| p.puuid != player.puuid && p.team_position == player.team_position)
        .unwrap()
}

fn get_team<'a>(
    match_info: &'a json::Match,
    player: &'a json::Participant,
) -> Vec<&'a json::Participant> {
    match_info
        .info
        .participants
        .iter()
        .filter(|p| p.team_id == player.team_id)
        .collect()
}

struct StatsAtMinuteGathering {
    cs_per_minute: f64,
    gold_diff: i32,
    cs_diff: i32,
    xp_diff: i32,
}

#[derive(Clone, Debug)]
struct StatsAtMinute {
    cs_per_minute: NumberWithOptionalDelta,
    gold_diff: NumberWithOptionalDelta,
    cs_diff: NumberWithOptionalDelta,
    xp_diff: NumberWithOptionalDelta,
}

impl StatsAtMinute {
    fn compare_to(&mut self, other: &Self) {
        self.cs_per_minute.compare_to(&other.cs_per_minute);
        self.gold_diff.compare_to(&other.gold_diff);
        self.cs_diff.compare_to(&other.cs_diff);
        self.xp_diff.compare_to(&other.xp_diff);
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, strum::EnumIter, strum::Display)]
enum Side {
    Blue,
    Red,
}

type HeatMapData = Vec<(Role, Side, usize, String)>;
type HeatMapDataGathering = HashMap<(Role, Side), HashMap<i64, Vec<json::Point>>>;

#[derive(Default)]
struct WeekStatsGathering {
    wins: u32,
    losses: u32,
    cs_per_minute: Vec<f64>,
    gold_share: Vec<f64>,
    champion_damage_share: Vec<f64>,
    objective_damage_share: Vec<f64>,
    vision_share: Vec<f64>,
    vision_score_per_minute: Vec<f64>,
    solo_kills: Vec<u32>,
    solo_deaths: Vec<u32>,
    stats_at: HashMap<u32, Vec<StatsAtMinuteGathering>>,
    heatmap_data: HeatMapDataGathering,
    roles: Vec<Role>,
    roles_sides: Vec<(Role, Side)>,
    wards_placed: Vec<(Position, TimeDelta)>,
}

#[derive(Clone, Debug)]
struct NumberWithOptionalDelta {
    number: f64,
    delta: Option<f64>,
    up_is_good: bool,
}

impl From<f64> for NumberWithOptionalDelta {
    fn from(number: f64) -> Self {
        let number = (number * 10.0).round() / 10.0;
        Self {
            number,
            delta: None,
            up_is_good: true,
        }
    }
}

impl NumberWithOptionalDelta {
    fn from_up_is_bad(number: f64) -> Self {
        let number = (number * 10.0).round() / 10.0;
        Self {
            number,
            delta: None,
            up_is_good: false,
        }
    }
    fn compare_to(&mut self, other: &Self) {
        self.delta = Some(self.number - other.number);
    }
    fn has_visible_diff(&self) -> Ordering {
        let mut factor = 10.0;
        if let Some(delta) = self.delta {
            if !self.up_is_good {
                factor = -factor;
            }
            #[allow(clippy::cast_possible_truncation)]
            ((delta * factor).round() as i64).cmp(&0)
        } else {
            Ordering::Equal
        }
    }
}

impl Display for NumberWithOptionalDelta {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.number.fmt(f)
    }
}

#[derive(Clone, Debug)]
struct WeekStats {
    number: i64,
    wins: u32,
    losses: u32,
    games_played: u32,
    winrate: NumberWithOptionalDelta,
    cs_per_minute: NumberWithOptionalDelta,
    gold_share: NumberWithOptionalDelta,
    champion_damage_share: NumberWithOptionalDelta,
    objective_damage_share: NumberWithOptionalDelta,
    vision_share: NumberWithOptionalDelta,
    vision_score_per_minute: NumberWithOptionalDelta,
    solo_kills: NumberWithOptionalDelta,
    solo_deaths: NumberWithOptionalDelta,
    at_minute_stats: Vec<(u32, StatsAtMinute)>,
    heatmap_data: HeatMapData,
}

impl WeekStats {
    fn compare_to(&mut self, other: &Self) {
        self.winrate.compare_to(&other.winrate);
        self.cs_per_minute.compare_to(&other.cs_per_minute);
        self.gold_share.compare_to(&other.gold_share);
        self.champion_damage_share
            .compare_to(&other.champion_damage_share);
        self.objective_damage_share
            .compare_to(&other.objective_damage_share);
        self.vision_share.compare_to(&other.vision_share);
        self.vision_score_per_minute
            .compare_to(&other.vision_score_per_minute);
        self.solo_kills.compare_to(&other.solo_kills);
        self.solo_deaths.compare_to(&other.solo_deaths);
        for (minute, stats) in &mut self.at_minute_stats {
            if let Some(other_stats) = other.at_minute_stats.iter().find(|(m, _)| m == minute) {
                stats.compare_to(&other_stats.1);
            }
        }
    }
}

#[derive(Template)]
#[template(path = "stats.html", escape = "none")]
struct DisplayData {
    player: Player,
    weeks: Vec<WeekStats>,
}

const NUM_WEEKS: i64 = 4;

#[allow(clippy::too_many_lines)]
async fn calc_stats(state: State, mut player: Player) -> Result<DisplayData> {
    let from = Utc::now() - chrono::Duration::weeks(NUM_WEEKS);
    debug!("Getting puuid");
    let puuid = get_puuid_and_canonical_name(&state, &mut player).await?;
    debug!("Getting match history");
    update_match_history(&state, &player, from).await?;
    debug!("Calculating stats");
    let now = Utc::now();
    let mut week_stats = state
        .matches_per_puuid
        .get(&puuid)
        .unwrap()
        .iter()
        .filter(|(_, m)| {
            m.info.game_mode == "CLASSIC"
                && m.info.game_duration > TimeDelta::minutes(5)
                && m.info.game_start_timestamp > from
        })
        .sorted_by_key(|(_, m)| m.info.game_start_timestamp)
        .chunk_by(|(_, m)| (now - m.info.game_start_timestamp).num_weeks())
        .into_iter()
        .map(|(weeks_ago, matches)| {
            let matches = matches.map(|(_, m)| m).collect::<Vec<_>>();
            let week_stats = matches.iter().fold(
                WeekStatsGathering::default(),
                |mut stats, m| {
                    let timeline = state.timeline_per_match.get(&m.metadata.match_id).unwrap();
                    let player = get_player(m, &puuid);
                    let player_team = get_team(m, player);
                    let opponent = get_opponent(m, player);

                    if player.win {
                        stats.wins += 1;
                    } else {
                        stats.losses += 1;
                    }

                    let cs_per_minute = player.total_minions_killed as f64
                        / m.info.game_duration.num_minutes() as f64;
                    stats.cs_per_minute.push(cs_per_minute);

                    let team_gold = player_team
                        .iter()
                        .map(|p| p.gold_earned as f64)
                        .sum::<f64>();
                    let gold_share = 100_f64 * player.gold_earned as f64 / team_gold;
                    stats.gold_share.push(gold_share);

                    let team_champion_damage = player_team
                        .iter()
                        .map(|p| p.total_damage_dealt_to_champions as f64)
                        .sum::<f64>();
                    let champion_damage_share = 100_f64
                        * player.total_damage_dealt_to_champions as f64
                        / team_champion_damage;
                    stats.champion_damage_share.push(champion_damage_share);

                    let team_objective_damage = player_team
                        .iter()
                        .map(|p| p.damage_dealt_to_objectives as f64)
                        .sum::<f64>();
                    let objective_damage_share =
                        100_f64 * player.damage_dealt_to_objectives as f64 / team_objective_damage;
                    stats.objective_damage_share.push(objective_damage_share);

                    let team_vision = player_team
                        .iter()
                        .map(|p| p.vision_score as f64)
                        .sum::<f64>();
                    let vision_share = 100_f64 * player.vision_score as f64 / team_vision;
                    stats.vision_share.push(vision_share);

                    let vision_score_per_minute =
                        player.vision_score as f64 / m.info.game_duration.num_minutes() as f64;
                    stats.vision_score_per_minute.push(vision_score_per_minute);

                    let timeline_player_id = timeline_get_player_id(&timeline, &puuid);
                    let timeline_opponent_id = timeline_get_player_id(&timeline, &opponent.puuid);
                    let team_other_player_ids = player_team
                        .iter()
                        .filter_map(|p| {
                            if p.puuid == puuid {
                                None
                            } else {
                                Some(timeline_get_player_id(&timeline, &p.puuid))
                            }
                        })
                        .collect::<Vec<_>>();

                    for minute in MINUTES_AT {
                        let stats_at = frame_stats_at(
                            &timeline.info.frames,
                            timeline_player_id,
                            timeline_opponent_id,
                            TimeDelta::minutes(minute as i64),
                        );
                        if let Some(stats_at) = stats_at {
                            stats.stats_at.entry(minute).or_default().push(stats_at);
                        }
                    }

                    let solo_kills = timeline
                        .info
                        .frames
                        .iter()
                        .map(|f| {
                            u32::try_from(f.events
                                .iter()
                                .filter(|e| {
                                    if let json::Event::ChampionKill(kill) = e {
                                        kill.killer_id == timeline_player_id
                                            && kill.assisting_participant_ids.is_empty()
                                    } else {
                                        false
                                    }
                                })
                                .count()).unwrap()
                        })
                        .sum::<u32>();
                    stats.solo_kills.push(solo_kills);

                    let deaths = timeline
                        .info
                        .frames
                        .iter()
                        .flat_map(|f| {
                            f.events.iter().filter_map(|e| {
                                if let json::Event::ChampionKill(kill) = e {
                                    if kill.victim_id == timeline_player_id {
                                        return Some((kill.timestamp, kill.position));
                                    }
                                }
                                None
                            })
                        })
                        .collect::<Vec<_>>();
                    let solo_deaths = deaths
                        .iter()
                        .filter(|(timestamp, position)| {
                            // Get the frames before and after the kill. Both should exist because there's the start and end of the game frames.
                            let frame_before = timeline
                                .info
                                .frames
                                .iter()
                                .filter(|f| f.timestamp < *timestamp)
                                .last()
                                .unwrap();
                            let frame_after = timeline
                                .info
                                .frames
                                .iter()
                                .find(|f| f.timestamp > *timestamp)
                                .unwrap();
                            // Check if the player was the only one in the area
                            for frame in [frame_before, frame_after] {
                                for pid in &team_other_player_ids {
                                    let other_player = frame.participant_frames.get(pid).unwrap();
                                    if other_player.position.distance(*position) < 4000.0 {
                                        return false;
                                    }
                                }
                            }
                            true
                        })
                        .count();
                    stats.solo_deaths.push(u32::try_from(solo_deaths).unwrap());

                    let side = match player.team_id {
                        100 => Side::Blue,
                        200 => Side::Red,
                        _ => unreachable!(),
                    };
                    let role = player.team_position;
                    stats.roles.push(role);
                    stats.roles_sides.push((role, side));
                    let heatmap_data = stats.heatmap_data.entry((role, side)).or_default();
                    for frame in &timeline.info.frames {
                        let minute = frame.timestamp.num_minutes();
                        let mut pos = frame
                            .participant_frames
                            .get(&timeline_player_id)
                            .unwrap()
                            .position;
                        pos.x /= 29;
                        pos.y /= 29;
                        pos.y = 512 - pos.y;
                        heatmap_data.entry(minute).or_default().push(pos);
                    }
                    stats
                },
            );

            let winrate = (100.0 * week_stats.wins as f64
                / (week_stats.wins + week_stats.losses) as f64)
                .into();
            let cs_per_minute = median_f64(&week_stats.cs_per_minute).into();
            let gold_share = median_f64(&week_stats.gold_share).into();
            let champion_damage_share = median_f64(&week_stats.champion_damage_share).into();
            let objective_damage_share = median_f64(&week_stats.objective_damage_share).into();
            let vision_share = median_f64(&week_stats.vision_share).into();
            let vision_score_per_minute = median_f64(&week_stats.vision_score_per_minute).into();
            let solo_kills = average(week_stats.solo_kills.iter().copied()).into();
            let solo_deaths = NumberWithOptionalDelta::from_up_is_bad(average(
                week_stats.solo_deaths.iter().copied(),
            ));
            let at_minute_stats = MINUTES_AT
                .iter()
                .filter_map(|&minute| {
                    let stats_at = week_stats.stats_at.get(&minute)?;
                    let cs_per_minute =
                        median_f64(stats_at.iter().map(|s| &s.cs_per_minute)).into();
                    let gold_diff = median(stats_at.iter().map(|s| s.gold_diff)).into();
                    let cs_diff = median(stats_at.iter().map(|s| s.cs_diff)).into();
                    let xp_diff = median(stats_at.iter().map(|s| s.xp_diff)).into();

                    Some((
                        minute,
                        StatsAtMinute {
                            cs_per_minute,
                            gold_diff,
                            cs_diff,
                            xp_diff,
                        },
                    ))
                })
                .collect();

            let role_counts = week_stats.roles.into_iter().counts();
            let role_side_counts = week_stats.roles_sides.into_iter().counts();
            let heatmap_data = week_stats
                .heatmap_data
                .into_iter()
                .filter_map(|((role, side), data)| {
                    let count = *role_side_counts.get(&(role, side)).unwrap();
                    if role == Role::None {
                        None
                    } else {
                        Some((role, side, count, serde_json::to_string(&data).unwrap()))
                    }
                })
                .sorted_unstable_by(|(role1, side1, _, _), (role2, side2, _, _)| {
                    let role1 = role_counts.get(role1).unwrap();
                    let &role2 = role_counts.get(role2).unwrap();
                    // Reverse to get most played at the top
                    role2.cmp(role1).then(match (side1, side2) {
                        (Side::Blue, Side::Red) => Ordering::Less,
                        (Side::Red, Side::Blue) => Ordering::Greater,
                        _ => Ordering::Equal,
                    })
                })
                .collect();
            Ok(WeekStats {
                number: NUM_WEEKS - weeks_ago,
                wins: week_stats.wins,
                losses: week_stats.losses,
                winrate,
                games_played: week_stats.wins + week_stats.losses,
                cs_per_minute,
                gold_share,
                champion_damage_share,
                objective_damage_share,
                vision_share,
                vision_score_per_minute,
                solo_kills,
                solo_deaths,
                at_minute_stats,
                heatmap_data,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut previous_week = None;
    for current_week in &mut week_stats {
        if let Some(previous_week) = previous_week {
            current_week.compare_to(&previous_week);
        }
        previous_week = Some(current_week.clone());
    }
    Ok(DisplayData {
        player,
        weeks: week_stats,
    })
}

#[get("/stats/{region}/{game_name}/{tag_line}")]
pub async fn page(state: State, path: web::Path<Player>) -> ActixResult<impl Responder> {
    let player = path.into_inner().normalized();
    debug!("Stats for {}", player);
    if let RedirectOrContinue::Redirect(redirect) = check_or_start_fetching(state.clone(), &player)
        .await
        .map_err(internal_server_error)?
    {
        return Ok(Either::Left(redirect));
    }
    Ok(Either::Right(
        calc_stats(state, player)
            .await
            .map_err(internal_server_error)?
            .customize()
            .insert_header(("content-type", "text/html")),
    ))
}
