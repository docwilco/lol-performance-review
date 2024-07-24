use crate::{
    fetcher::{check_or_start_fetching, RedirectOrContinue},
    internal_server_error, normalize_champion_name,
    riot_api::{
        get_puuid_and_canonical_name,
        json::{self, Match, Role},
        update_match_history,
    },
    Player, PlayerRoleChamp, Result, State,
};
use actix_web::{routes, web, Either, Responder, Result as ActixResult};
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
include!(concat!(env!("OUT_DIR"), "/codegen-champ-names.rs"));

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

const XP_LEVELS: [i32; 17] = [
    280, 380, 480, 580, 680, 780, 880, 980, 1080, 1180, 1280, 1380, 1480, 1580, 1680, 1780, 1880,
];

fn level_for_xp(mut xp: i32) -> f64 {
    let mut level = 1.0;
    // Because we use the limited array above, the result will never be more than 18.0
    for &xp_level in &XP_LEVELS {
        if xp >= xp_level {
            level += 1.0;
        } else {
            level += (xp as f64) / (xp_level as f64);
            break;
        }
        xp -= xp_level;
    }
    level
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
    let player_level = level_for_xp(frame.participant_frames.get(&player)?.xp);
    let opponent_level = level_for_xp(frame.participant_frames.get(&opponent)?.xp);
    let level_diff = player_level - opponent_level;
    Some(StatsAtMinuteGathering {
        cs_per_minute: cpm,
        gold_diff,
        cs_diff,
        level_diff,
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
    level_diff: f64,
}

#[derive(Clone, Debug)]
struct StatsAtMinute {
    cs_per_minute: NumberWithOptionalDelta,
    gold_diff: NumberWithOptionalDelta,
    cs_diff: NumberWithOptionalDelta,
    level_diff: NumberWithOptionalDelta,
}

impl StatsAtMinute {
    fn compare_to(&mut self, other: &Self) {
        self.cs_per_minute.compare_to(&other.cs_per_minute);
        self.gold_diff.compare_to(&other.gold_diff);
        self.cs_diff.compare_to(&other.cs_diff);
        self.level_diff.compare_to(&other.level_diff);
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
    kills: Vec<i32>,
    deaths: Vec<i32>,
    assists: Vec<i32>,
    kda: Vec<f64>,
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
    _wards_placed: Vec<(Position, TimeDelta)>,
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
    kills: NumberWithOptionalDelta,
    deaths: NumberWithOptionalDelta,
    assists: NumberWithOptionalDelta,
    kda: NumberWithOptionalDelta,
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
    per_role_per_champ: Vec<(Role, Vec<(String, String, WeekStats)>)>,
}

impl WeekStats {
    fn compare_to(&mut self, other: &Self) {
        self.winrate.compare_to(&other.winrate);
        self.kills.compare_to(&other.kills);
        self.deaths.compare_to(&other.deaths);
        self.assists.compare_to(&other.assists);
        self.kda.compare_to(&other.kda);
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
        for (role, per_champ) in &mut self.per_role_per_champ {
            if let Some((_, other_per_champ)) =
                other.per_role_per_champ.iter().find(|(r, _)| r == role)
            {
                for (champ, _, champ_stats) in per_champ {
                    if let Some((_, _, other_champ_stats)) =
                        other_per_champ.iter().find(|(c, _, _)| c == champ)
                    {
                        champ_stats.compare_to(other_champ_stats);
                    }
                }
            }
        }
    }
}

fn team_share<'a>(
    player: &'a json::Participant,
    team: impl IntoIterator<Item = &'a &'a json::Participant>,
    field: impl Fn(&'a json::Participant) -> i32,
) -> f64 {
    let team_field = team.into_iter().map(|p| field(p) as f64).sum::<f64>();
    100.0 * (field(player) as f64) / team_field
}

fn solo_kills(timeline: &json::Timeline, player_id: usize) -> u32 {
    timeline
        .info
        .frames
        .iter()
        .map(|f| {
            u32::try_from(
                f.events
                    .iter()
                    .filter(|e| {
                        if let json::Event::ChampionKill(kill) = e {
                            kill.killer_id == player_id && kill.assisting_participant_ids.is_empty()
                        } else {
                            false
                        }
                    })
                    .count(),
            )
            .unwrap()
        })
        .sum::<u32>()
}

fn solo_deaths<'a>(
    timeline: &'a json::Timeline,
    team: impl IntoIterator<Item = &'a &'a json::Participant>,
    player_id: usize,
    puuid: &'a str,
) -> u32 {
    let team_other_player_ids = team
        .into_iter()
        .filter_map(|p| {
            if p.puuid == puuid {
                None
            } else {
                Some(timeline_get_player_id(timeline, &p.puuid))
            }
        })
        .collect::<Vec<_>>();

    let deaths = timeline
        .info
        .frames
        .iter()
        .flat_map(|f| {
            f.events.iter().filter_map(|e| {
                if let json::Event::ChampionKill(kill) = e {
                    if kill.victim_id == player_id {
                        return Some((kill.timestamp, kill.position));
                    }
                }
                None
            })
        })
        .collect::<Vec<_>>();

    deaths
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
        .count()
        .try_into()
        .unwrap()
}

fn gather_stats<'a>(
    state: &State,
    matches: impl IntoIterator<Item = &'a &'a json::Match>,
    puuid: &str,
) -> WeekStatsGathering {
    matches
        .into_iter()
        .fold(WeekStatsGathering::default(), |mut stats, m| {
            let timeline = state.timeline_per_match.get(&m.metadata.match_id).unwrap();
            let player = get_player(m, puuid);
            let team = get_team(m, player);
            let opponent = get_opponent(m, player);

            if player.win {
                stats.wins += 1;
            } else {
                stats.losses += 1;
            }

            stats.kills.push(player.kills);
            stats.deaths.push(player.deaths);
            stats.assists.push(player.assists);
            let deaths = (player.deaths as f64).max(1.0);
            stats
                .kda
                .push((player.kills as f64 + player.assists as f64) / deaths);

            let cs_per_minute =
                player.total_minions_killed as f64 / m.info.game_duration.num_minutes() as f64;
            stats.cs_per_minute.push(cs_per_minute);

            stats
                .gold_share
                .push(team_share(player, &team, |p| p.gold_earned));

            stats
                .champion_damage_share
                .push(team_share(player, &team, |p| {
                    p.total_damage_dealt_to_champions
                }));

            stats
                .objective_damage_share
                .push(team_share(player, &team, |p| p.damage_dealt_to_objectives));

            stats
                .vision_share
                .push(team_share(player, &team, |p| p.vision_score));

            let vision_score_per_minute =
                player.vision_score as f64 / m.info.game_duration.num_minutes() as f64;
            stats.vision_score_per_minute.push(vision_score_per_minute);

            let timeline_player_id = timeline_get_player_id(&timeline, puuid);
            let timeline_opponent_id = timeline_get_player_id(&timeline, &opponent.puuid);

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

            stats
                .solo_kills
                .push(solo_kills(&timeline, timeline_player_id));

            stats
                .solo_deaths
                .push(solo_deaths(&timeline, &team, timeline_player_id, puuid));

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
        })
}

fn convert_stats(week_number: i64, gathered: WeekStatsGathering) -> WeekStats {
    let at_minute_stats = MINUTES_AT
        .iter()
        .filter_map(|&minute| {
            let stats_at = gathered.stats_at.get(&minute)?;
            let cs_per_minute = median_f64(stats_at.iter().map(|s| &s.cs_per_minute)).into();
            let gold_diff = median(stats_at.iter().map(|s| s.gold_diff)).into();
            let cs_diff = median(stats_at.iter().map(|s| s.cs_diff)).into();
            let level_diff = median_f64(stats_at.iter().map(|s| &s.level_diff)).into();

            Some((
                minute,
                StatsAtMinute {
                    cs_per_minute,
                    gold_diff,
                    cs_diff,
                    level_diff,
                },
            ))
        })
        .collect();

    let role_counts = gathered.roles.into_iter().counts();
    let role_side_counts = gathered.roles_sides.into_iter().counts();
    let heatmap_data = gathered
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
    WeekStats {
        number: week_number,
        wins: gathered.wins,
        losses: gathered.losses,
        winrate: (100.0 * gathered.wins as f64 / (gathered.wins + gathered.losses) as f64).into(),
        games_played: gathered.wins + gathered.losses,
        kills: average(gathered.kills.iter().copied()).into(),
        deaths: NumberWithOptionalDelta::from_up_is_bad(average(gathered.deaths.iter().copied())),
        assists: average(gathered.assists.iter().copied()).into(),
        kda: _average_f64(&gathered.kda).into(),
        cs_per_minute: median_f64(&gathered.cs_per_minute).into(),
        gold_share: median_f64(&gathered.gold_share).into(),
        champion_damage_share: median_f64(&gathered.champion_damage_share).into(),
        objective_damage_share: median_f64(&gathered.objective_damage_share).into(),
        vision_share: median_f64(&gathered.vision_share).into(),
        vision_score_per_minute: median_f64(&gathered.vision_score_per_minute).into(),
        solo_kills: average(gathered.solo_kills.iter().copied()).into(),
        solo_deaths: NumberWithOptionalDelta::from_up_is_bad(average(
            gathered.solo_deaths.iter().copied(),
        )),
        at_minute_stats,
        heatmap_data,
        per_role_per_champ: vec![],
    }
}

fn matches_by_role_lane<'a>(
    matches: impl IntoIterator<Item = &'a &'a json::Match>,
    puuid: &'a str,
) -> Vec<(Role, Vec<(String, Vec<&Match>)>)> {
    let mut map = HashMap::new();
    for m in matches {
        let player = get_player(m, puuid);
        let role = player.team_position;
        let champion = player.champion_name.clone();
        map.entry(role)
            .or_insert_with(HashMap::new)
            .entry(champion)
            .or_insert_with(Vec::new)
            .push(*m);
    }
    map.into_iter()
        .map(|(role, champ_map)| {
            let champ_map = champ_map
                .into_iter()
                .sorted_unstable_by_key(|(_, matches)| {
                    #[allow(clippy::cast_possible_truncation)]
                    #[allow(clippy::cast_possible_wrap)]
                    -(matches.len() as i32)
                })
                .collect::<Vec<_>>();
            (role, champ_map)
        })
        .sorted_by_cached_key(|(_, champ_map)| {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_possible_wrap)]
            -(champ_map
                .iter()
                .map(|(_, matches)| matches.len())
                .sum::<usize>() as i32)
        })
        .collect()
}

#[derive(Template)]
#[template(path = "stats.html", escape = "none")]
struct DisplayData {
    player: Player,
    champion: Option<String>,
    weeks: Vec<WeekStats>,
}

const NUM_WEEKS: i64 = 4;

#[allow(clippy::too_many_lines)]
async fn calc_stats(
    state: State,
    mut player: Player,
    role: Option<Role>,
    champion: Option<&str>,
) -> Result<DisplayData> {
    let from = Utc::now() - chrono::Duration::weeks(NUM_WEEKS);
    debug!("Getting puuid");
    let puuid = get_puuid_and_canonical_name(&state, &mut player).await?;
    debug!("Getting match history");
    update_match_history(&state, &player, from).await?;
    debug!("Calculating stats");
    let now = Utc::now();
    let player_matches = state.matches_per_puuid.get(&puuid).unwrap();
    let mut week_stats = player_matches
        .iter()
        .filter(|(_, m)| {
            let champ_match = champion.map_or(true, |champion| {
                m.info.participants.iter().any(|p| {
                    p.puuid == puuid && normalize_champion_name(&p.champion_name) == champion
                })
            });
            let role_match = role.map_or(true, |role| {
                m.info
                    .participants
                    .iter()
                    .any(|p| p.puuid == puuid && p.team_position == role)
            });
            debug!("Champ match: {champ_match} Role match: {role_match}");
            champ_match
                && role_match
                && m.info.game_mode == "CLASSIC"
                && m.info.game_duration > TimeDelta::minutes(5)
                && m.info.game_start_timestamp > from
        })
        .sorted_by_key(|(_, m)| m.info.game_start_timestamp)
        .chunk_by(|(_, m)| (now - m.info.game_start_timestamp).num_weeks())
        .into_iter()
        .map(|(weeks_ago, matches)| {
            let matches = matches.map(|(_, m)| m).collect::<Vec<_>>();
            let week_stats = gather_stats(&state, &matches, &puuid);
            let mut display_stats = convert_stats(NUM_WEEKS - weeks_ago, week_stats);
            if champion.is_none() {
                display_stats.per_role_per_champ = matches_by_role_lane(&matches, &puuid)
                    .into_iter()
                    .map(|(role, champ_map)| {
                        (
                            role,
                            champ_map
                                .into_iter()
                                .map(|(champ, champ_matches)| {
                                    let normalized_champ = normalize_champion_name(&champ);
                                    let role_champ_stats =
                                        gather_stats(&state, &champ_matches, &puuid);
                                    let role_champ_display_stats =
                                        convert_stats(NUM_WEEKS - weeks_ago, role_champ_stats);
                                    (champ, normalized_champ, role_champ_display_stats)
                                })
                                .collect(),
                        )
                    })
                    .collect();
            }
            display_stats
        })
        .collect::<Vec<_>>();

    let mut previous_week = None;
    for current_week in &mut week_stats {
        if let Some(previous_week) = previous_week {
            current_week.compare_to(&previous_week);
        }
        previous_week = Some(current_week.clone());
    }
    Ok(DisplayData {
        player,
        champion: champion.map(|c| (*CHAMP_NAMES.get(c).unwrap()).to_string()),
        weeks: week_stats,
    })
}

#[routes]
#[get("/stats/{region}/{game_name}/{tag_line}")]
#[get("/stats/{region}/{game_name}/{tag_line}/{role}/{champion}")]
pub async fn page(state: State, path: web::Path<PlayerRoleChamp>) -> ActixResult<impl Responder> {
    let (player, role, champion) = path.into_inner().into();
    if let RedirectOrContinue::Redirect(redirect) =
        check_or_start_fetching(state.clone(), &player, role, champion.as_deref())
            .await
            .map_err(internal_server_error)?
    {
        return Ok(Either::Left(redirect));
    }
    Ok(Either::Right(
        calc_stats(state, player, role, champion.as_deref())
            .await
            .map_err(internal_server_error)?
            .customize()
            .insert_header(("content-type", "text/html")),
    ))
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::level_for_xp;
    #[test_case(0, 1.0)]
    #[test_case(280, 2.0)]
    #[test_case(660, 3.0)]
    #[test_case(1140, 4.0)]
    #[test_case(1720, 5.0)]
    #[test_case(2400, 6.0)]
    #[test_case(3180, 7.0)]
    #[test_case(4060, 8.0)]
    #[test_case(5040, 9.0)]
    #[test_case(6120, 10.0)]
    #[test_case(7300, 11.0)]
    #[test_case(8580, 12.0)]
    #[test_case(9960, 13.0)]
    #[test_case(11440, 14.0)]
    #[test_case(13020, 15.0)]
    #[test_case(14700, 16.0)]
    #[test_case(16480, 17.0)]
    #[test_case(18360, 18.0)]
    #[test_case(140, 1.5)]
    #[test_case(900, 3.5)]
    #[test_case(999_999, 18.0)]
    fn test_level_for_xp(xp: i32, level: f64) {
        assert!((level_for_xp(xp) - level).abs() < 0.01);
    }
}
