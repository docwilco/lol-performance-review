use crate::{
    normalize_champion_name,
    riot_api::{
        get_puuid_and_canonical_name,
        json::{self, Match, Role},
        update_match_history,
    },
    Player, Result, State,
};
use chrono::{TimeDelta, Utc};
use itertools::{Itertools, Position};
use log::debug;
use ordered_float::OrderedFloat;
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt::{self, Display, Formatter, Write},
    ops::RangeInclusive,
};

const NUM_WEEKS: i64 = 4;
const MINUTES_AT: RangeInclusive<u32> = 2..=20;
const XP_LEVELS: [i32; 17] = [
    280, 380, 480, 580, 680, 780, 880, 980, 1080, 1180, 1280, 1380, 1480, 1580, 1680, 1780, 1880,
];
include!(concat!(env!("OUT_DIR"), "/codegen-item-ranks.rs"));

struct StatsAtMinuteGathering {
    cs_per_minute: f64,
    gold_diff: i32,
    cs_diff: i32,
    level_diff: f64,
}

#[derive(Clone, Debug)]
pub struct StatsAtMinute {
    pub cs_per_minute: f64,
    pub gold_diff: f64,
    pub cs_diff: f64,
    pub level_diff: f64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, strum::EnumIter, strum::Display)]
pub enum Side {
    Blue,
    Red,
}

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
    legendary_item_buy_times: Vec<Vec<TimeDelta>>,
}

#[derive(Clone, Debug)]
pub struct NumberWithOptionalDelta {
    pub number: f64,
    pub delta: Option<f64>,
    pub up_is_good: bool,
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
    fn up_is_bad_from(number: f64) -> Self {
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
    pub fn has_visible_diff(&self) -> Ordering {
        let mut factor = 10.0;
        if let Some(delta) = self.delta {
            if !self.up_is_good {
                factor = -factor;
            }
            let truncated = (delta * factor).round();
            if truncated.abs() >= 1.0 {
                return truncated.partial_cmp(&0.0).unwrap();
            }
        }
        Ordering::Equal
    }
}

impl Display for NumberWithOptionalDelta {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.number.fmt(f)
    }
}

#[derive(Clone, Debug)]
pub struct DisplayTimeDelta {
    pub time: TimeDelta,
    pub delta: Option<TimeDelta>,
    pub down_is_good: bool,
}

impl From<TimeDelta> for DisplayTimeDelta {
    fn from(time: TimeDelta) -> Self {
        Self {
            time,
            delta: None,
            down_is_good: true,
        }
    }
}

impl DisplayTimeDelta {
    fn compare_to(&mut self, other: &Self) {
        self.delta = Some(self.time - other.time);
    }
    pub fn has_visible_diff(&self) -> Ordering {
        if let Some(mut delta) = self.delta {
            if self.down_is_good {
                delta = -delta;
            }
            if delta.num_seconds().abs() >= 1 {
                return delta.partial_cmp(&TimeDelta::zero()).unwrap();
            }
        }
        Ordering::Equal
    }
    pub fn display_diff(&self) -> String {
        if let Some(delta) = self.delta {
            let mut seconds = delta.num_seconds();
            let mut result = String::new();
            result.push(if seconds < 0 { '-' } else { '+' });
            seconds = seconds.abs();
            let hours = seconds / 3600;
            seconds %= 3600;
            let minutes = seconds / 60;
            seconds %= 60;
            if hours > 0 {
                write!(result, "{hours}h").unwrap();
            }
            if minutes > 0 {
                write!(result, "{minutes}m").unwrap();
            }
            write!(result, "{seconds}s").unwrap();
            result
        } else {
            String::new()
        }
    }
}

impl Display for DisplayTimeDelta {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut seconds = self.time.num_seconds();
        let hours = seconds / 3600;
        seconds %= 3600;
        let minutes = seconds / 60;
        seconds %= 60;
        if hours > 0 {
            write!(f, "{hours}h")?;
        }
        if minutes > 0 {
            write!(f, "{minutes}m")?;
        }
        write!(f, "{seconds}s")
    }
}

#[derive(Clone, Debug)]
pub struct GroupStats {
    pub title: String,
    pub id: String,
    pub wins: u32,
    pub losses: u32,
    pub games_played: u32,
    pub kills: NumberWithOptionalDelta,
    pub deaths: NumberWithOptionalDelta,
    pub assists: NumberWithOptionalDelta,
    pub kda: NumberWithOptionalDelta,
    pub winrate: NumberWithOptionalDelta,
    pub cs_per_minute: NumberWithOptionalDelta,
    pub gold_share: NumberWithOptionalDelta,
    pub champion_damage_share: NumberWithOptionalDelta,
    pub objective_damage_share: NumberWithOptionalDelta,
    pub vision_share: NumberWithOptionalDelta,
    pub vision_score_per_minute: NumberWithOptionalDelta,
    pub solo_kills: NumberWithOptionalDelta,
    pub solo_deaths: NumberWithOptionalDelta,
    pub at_minute_stats: Vec<(u32, StatsAtMinute)>,
    pub previous_at_minute_stats: Option<Vec<(u32, StatsAtMinute)>>,
    pub heatmap_data: HeatMapData,
    #[allow(clippy::type_complexity)]
    pub legendary_buy_times: Vec<DisplayTimeDelta>,
    pub per_role_per_champ: Vec<(Role, DisplayChampMatches)>,
    pub per_role_per_enemy: Vec<(Role, DisplayChampMatches)>,
}

impl GroupStats {
    pub fn compare_to(&mut self, other: &Self) {
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
        self.legendary_buy_times
            .iter_mut()
            .zip(&other.legendary_buy_times)
            .for_each(|(buy_times, other_buy_times)| {
                buy_times.compare_to(other_buy_times);
            });
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
        for (role, per_champ) in &mut self.per_role_per_enemy {
            if let Some((_, other_per_champ)) =
                other.per_role_per_enemy.iter().find(|(r, _)| r == role)
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

pub type HeatMapData = Vec<(Role, Side, usize, String)>;
type HeatMapDataGathering = HashMap<(Role, Side), HashMap<i64, Vec<json::Point>>>;
type ChampMatches<'a> = Vec<(String, Vec<&'a Match>)>;
type DisplayChampMatches = Vec<(String, String, GroupStats)>;

fn median<'a, T>(values: impl IntoIterator<Item = &'a T>) -> f64
where
    T: Copy + Into<f64> + 'a,
{
    let mut values = values
        .into_iter()
        .map(|&value| OrderedFloat::from(value.into()))
        .collect::<Vec<_>>();
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1].into_inner() + values[mid].into_inner()) / 2.0
    } else {
        values[mid].into_inner()
    }
}

fn median_td<'a>(values: impl IntoIterator<Item = &'a TimeDelta>) -> TimeDelta {
    let mut values = values
        .into_iter()
        .map(TimeDelta::num_seconds)
        .collect::<Vec<_>>();
    values.sort_unstable();
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        TimeDelta::seconds((values[mid - 1] + values[mid]) / 2)
    } else {
        TimeDelta::seconds(values[mid])
    }
}

fn average<'a, T>(values: impl IntoIterator<Item = &'a T>) -> f64
where
    T: Copy + Into<f64> + 'a,
{
    let mut sum = 0.0;
    let mut count = 0;
    for value in values {
        sum += Into::<f64>::into(*value);
        count += 1;
    }
    sum / f64::from(count)
}

fn level_for_xp(mut xp: i32) -> f64 {
    let mut level = 1.0;
    // Because we use the limited array above, the result will never be more than 18.0
    for &xp_level in &XP_LEVELS {
        if xp >= xp_level {
            level += 1.0;
        } else {
            level += f64::from(xp) / f64::from(xp_level);
            break;
        }
        xp -= xp_level;
    }
    level
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

fn frame_stats_at(
    frames: &[json::Frame],
    player: usize,
    opponent: usize,
    timestamp: TimeDelta,
) -> Option<StatsAtMinuteGathering> {
    let frame = frames.iter().find(|f| f.timestamp >= timestamp)?;
    let cpm = (f64::from(frame.participant_frames.get(&player)?.minions_killed)
        + f64::from(frame.participant_frames.get(&player)?.jungle_minions_killed))
        / f64::from(i32::try_from(timestamp.num_minutes()).unwrap());
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

fn team_share<'a>(
    player: &'a json::Participant,
    team: impl IntoIterator<Item = &'a &'a json::Participant>,
    field: impl Fn(&'a json::Participant) -> i32,
) -> f64 {
    let team_field = team.into_iter().map(|p| f64::from(field(p))).sum::<f64>();
    100.0 * f64::from(field(player)) / team_field
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
            let deaths = f64::from(player.deaths).max(1.0);
            stats
                .kda
                .push((f64::from(player.kills) + f64::from(player.assists)) / deaths);

            let cs_per_minute = f64::from(player.total_minions_killed)
                / f64::from(i32::try_from(m.info.game_duration.num_minutes()).unwrap());
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

            let vision_score_per_minute = f64::from(player.vision_score)
                / f64::from(i32::try_from(m.info.game_duration.num_minutes()).unwrap());
            stats.vision_score_per_minute.push(vision_score_per_minute);

            let timeline_player_id = timeline_get_player_id(&timeline, puuid);
            let timeline_opponent_id = timeline_get_player_id(&timeline, &opponent.puuid);

            for minute in MINUTES_AT {
                let stats_at = frame_stats_at(
                    &timeline.info.frames,
                    timeline_player_id,
                    timeline_opponent_id,
                    TimeDelta::minutes(i64::from(minute)),
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

            add_legendary_buys(&mut stats, &timeline, timeline_player_id);
            stats
        })
}

fn get_legendary_buys<'a>(
    frames: impl IntoIterator<Item = &'a json::Frame>,
    timeline_player_id: usize,
) -> Vec<TimeDelta> {
    // Get timestamps of all legendary buys and sells
    let int = frames
        .into_iter()
        .flat_map(|f| {
            f.events.iter().filter_map(|e| {
                if let json::Event::ItemPurchased {
                    item_id,
                    participant_id,
                    timestamp,
                } = e
                {
                    if *participant_id == timeline_player_id
                        && ITEM_RANKS.get(item_id) == Some(&json::ItemType::Legendary)
                    {
                        return Some((*timestamp, true));
                    }
                }
                if let json::Event::ItemSold {
                    item_id,
                    participant_id,
                    timestamp,
                } = e
                {
                    if *participant_id == timeline_player_id
                        && ITEM_RANKS.get(item_id) == Some(&json::ItemType::Legendary)
                    {
                        return Some((*timestamp, false));
                    }
                }
                if let json::Event::ItemUndo {
                    after_id: _,
                    before_id,
                    gold_gain: _,
                    participant_id,
                    timestamp,
                } = e
                {
                    if *participant_id == timeline_player_id
                        && ITEM_RANKS.get(before_id) == Some(&json::ItemType::Legendary)
                    {
                        return Some((*timestamp, false));
                    }
                }
                None
            })
        })
        .collect::<Vec<_>>();
    // Then remove the buys that are followed by a sell
    int.into_iter()
        .fold(vec![], |mut legendary_buys, (timestamp, is_buy)| {
            if is_buy {
                legendary_buys.push(timestamp);
            } else {
                // Can't sell a legendary item if you haven't bought one, so this
                // should always work.
                legendary_buys.pop().unwrap();
            }
            legendary_buys
        })
}

fn add_legendary_buys(
    stats: &mut WeekStatsGathering,
    timeline: &json::Timeline,
    timeline_player_id: usize,
) {
    let this_games_buys = get_legendary_buys(&timeline.info.frames, timeline_player_id);
    while stats.legendary_item_buy_times.len() < this_games_buys.len() {
        stats.legendary_item_buy_times.push(vec![]);
    }
    stats
        .legendary_item_buy_times
        .iter_mut()
        .zip(this_games_buys)
        .for_each(|(buys, timestamp)| {
            buys.push(timestamp);
        });
}

fn convert_stats(title: &str, gathered: WeekStatsGathering) -> GroupStats {
    let at_minute_stats = MINUTES_AT
        .filter_map(|minute| {
            let stats_at = gathered.stats_at.get(&minute)?;
            let cs_per_minute = median(stats_at.iter().map(|s| &s.cs_per_minute));
            let gold_diff = median(stats_at.iter().map(|s| &s.gold_diff));
            let cs_diff = median(stats_at.iter().map(|s| &s.cs_diff));
            let level_diff = median(stats_at.iter().map(|s| &s.level_diff));

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
    let legendary_buy_times = gathered
        .legendary_item_buy_times
        .into_iter()
        .map(|nth| DisplayTimeDelta::from(median_td(&nth)))
        .collect();
    GroupStats {
        title: title.to_string(),
        id: title.to_lowercase().replace(' ', ""),
        wins: gathered.wins,
        losses: gathered.losses,
        winrate: (100.0 * f64::from(gathered.wins) / f64::from(gathered.wins + gathered.losses))
            .into(),
        games_played: gathered.wins + gathered.losses,
        kills: average(&gathered.kills).into(),
        deaths: NumberWithOptionalDelta::up_is_bad_from(average(&gathered.deaths)),
        assists: average(&gathered.assists).into(),
        kda: average(&gathered.kda).into(),
        cs_per_minute: median(&gathered.cs_per_minute).into(),
        gold_share: median(&gathered.gold_share).into(),
        champion_damage_share: median(&gathered.champion_damage_share).into(),
        objective_damage_share: median(&gathered.objective_damage_share).into(),
        vision_share: median(&gathered.vision_share).into(),
        vision_score_per_minute: median(&gathered.vision_score_per_minute).into(),
        solo_kills: average(&gathered.solo_kills).into(),
        solo_deaths: NumberWithOptionalDelta::up_is_bad_from(average(&gathered.solo_deaths)),
        at_minute_stats,
        previous_at_minute_stats: None,
        heatmap_data,
        legendary_buy_times,
        per_role_per_champ: vec![],
        per_role_per_enemy: vec![],
    }
}

fn matches_by_role_champ<'a>(
    matches: impl IntoIterator<Item = &'a &'a json::Match>,
    puuid: &'a str,
) -> Vec<(Role, ChampMatches)> {
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
                // This sorts by the number of matches played with the champion, highest first
                .sorted_unstable_by_key(|(_, matches)| -(i32::try_from(matches.len()).unwrap()))
                .collect::<Vec<_>>();
            (role, champ_map)
        })
        // This sorts by the number of matches played in the role, highest first
        .sorted_by_cached_key(|(_, champ_map)| {
            -(i32::try_from(
                champ_map
                    .iter()
                    .map(|(_, matches)| matches.len())
                    .sum::<usize>(),
            )
            .unwrap())
        })
        .collect()
}

fn matches_by_role_enemy<'a>(
    matches: impl IntoIterator<Item = &'a &'a json::Match>,
    puuid: &'a str,
) -> Vec<(Role, ChampMatches)> {
    let mut map = HashMap::new();
    for m in matches {
        let player = get_player(m, puuid);
        let opponent = get_opponent(m, player);
        let role = player.team_position;
        let enemy = opponent.champion_name.clone();
        map.entry(role)
            .or_insert_with(HashMap::new)
            .entry(enemy)
            .or_insert_with(Vec::new)
            .push(*m);
    }
    map.into_iter()
        .map(|(role, champ_map)| {
            let champ_map = champ_map
                .into_iter()
                // This sorts by the number of matches played against the champion, highest first
                .sorted_unstable_by_key(|(_, matches)| -(i32::try_from(matches.len()).unwrap()))
                // Now sort stable by the winrate, stable so that the previous sort is not affected when the winrate is the same
                .sorted_by_cached_key(|(_, matches)| {
                    let wins = matches
                        .iter()
                        .filter(|m| {
                            m.info
                                .participants
                                .iter()
                                .any(|p| p.puuid == puuid && p.win)
                        })
                        .count();
                    let wins = i32::try_from(wins).unwrap();
                    let len = i32::try_from(matches.len()).unwrap();
                    let winrate = 100.0 * f64::from(wins) / f64::from(len);
                    OrderedFloat(winrate)
                })
                .collect::<Vec<_>>();
            (role, champ_map)
        })
        // This sorts by the number of matches played in the role, highest first
        .sorted_by_cached_key(|(_, champ_map)| {
            -(i32::try_from(
                champ_map
                    .iter()
                    .map(|(_, matches)| matches.len())
                    .sum::<usize>(),
            )
            .unwrap())
        })
        .collect()
}

pub async fn calc_stats(
    state: State,
    player: &mut Player,
    role: Option<Role>,
    champion: Option<&str>,
) -> Result<Vec<GroupStats>> {
    let from = Utc::now() - chrono::Duration::weeks(NUM_WEEKS);
    debug!("Getting puuid");
    let puuid = get_puuid_and_canonical_name(&state, player).await?;
    debug!("Getting match history");
    update_match_history(&state, player, from).await?;
    debug!("Calculating stats");
    let now = Utc::now();
    let player_matches = state.matches_per_puuid.get(&puuid).unwrap();
    let all_matches = player_matches.iter().filter_map(|(_, m)| {
        let champ_match = champion.map_or(true, |champion| {
            m.info
                .participants
                .iter()
                .any(|p| p.puuid == puuid && normalize_champion_name(&p.champion_name) == champion)
        });
        let role_match = role.map_or(true, |role| {
            m.info
                .participants
                .iter()
                .any(|p| p.puuid == puuid && p.team_position == role)
        });
        if champ_match
            && role_match
            && m.info.game_mode == "CLASSIC"
            && m.info.game_duration > TimeDelta::minutes(5)
            && m.info.game_start_timestamp > from {
            Some(m)
        } else {
            None
        }
    }).collect::<Vec<_>>();
    let mut group_stats = all_matches.clone()
        .into_iter()
        .sorted_by_key(|m| m.info.game_start_timestamp)
        .chunk_by(|m| (now - m.info.game_start_timestamp).num_weeks())
        .into_iter()
        .map(|(weeks_ago, matches)| {
            let matches = matches.collect::<Vec<_>>();
            let week_stats = gather_stats(&state, &matches, &puuid);
            let title = format!("Week {}", NUM_WEEKS - weeks_ago);
            let mut display_stats = convert_stats(&title, week_stats);
            if champion.is_none() {
                display_stats.per_role_per_champ =
                    per_role_per_champ(&matches, &puuid, role, &state, &title);
            }
            display_stats.per_role_per_enemy =
                per_role_per_enemy(&matches, &puuid, role, &state, &title);

            display_stats
        })
        .collect::<Vec<_>>();
    let totals = gather_stats(&state, &all_matches, &puuid);
    let title = "Total".to_string();
    let mut total_stats = convert_stats(&title, totals);
    if champion.is_none() {
        total_stats.per_role_per_champ =
            per_role_per_champ(&all_matches, &puuid, role, &state, &title);
    }
    total_stats.per_role_per_enemy =
        per_role_per_enemy(&all_matches, &puuid, role, &state, &title);
    group_stats.push(total_stats);
    Ok(group_stats)
}

fn per_role_per_enemy<'a>(
    matches: impl IntoIterator<Item = &'a &'a json::Match>,
    puuid: &'a str,
    role: Option<Role>,
    state: &State,
    title: &str,
) -> Vec<(Role, DisplayChampMatches)> {
    matches_by_role_enemy(matches, puuid)
        .into_iter()
        .filter_map(|(by_role, champ_map)| {
            if role.is_some() && role != Some(by_role) {
                return None;
            }
            Some((
                by_role,
                champ_map
                    .into_iter()
                    .map(|(enemy, enemy_matches)| {
                        let normalized_enemy = normalize_champion_name(&enemy);
                        let role_enemy_stats = gather_stats(state, &enemy_matches, puuid);
                        let role_enemy_display_stats =
                            convert_stats(title, role_enemy_stats);
                        (enemy, normalized_enemy, role_enemy_display_stats)
                    })
                    .collect(),
            ))
        })
        .collect()
}

fn per_role_per_champ(
    matches: &Vec<&Match>,
    puuid: &str,
    role: Option<Role>,
    state: &State,
    title: &str,
) -> Vec<(Role, DisplayChampMatches)> {
    matches_by_role_champ(matches, puuid)
        .into_iter()
        .filter_map(|(by_role, champ_map)| {
            if role.is_some() && role != Some(by_role) {
                return None;
            }
            Some((
                by_role,
                champ_map
                    .into_iter()
                    .map(|(champ, champ_matches)| {
                        let normalized_champ = normalize_champion_name(&champ);
                        let role_champ_stats = gather_stats(state, &champ_matches, puuid);
                        let role_champ_display_stats =
                            convert_stats(title, role_champ_stats);
                        (champ, normalized_champ, role_champ_display_stats)
                    })
                    .collect(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::level_for_xp;
    use crate::riot_api::json;
    use chrono::TimeDelta;
    use std::collections::HashMap;
    use test_case::test_case;

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

    #[test_case(&[1_u32, 2, 3, 4, 5], 3.0)]
    #[test_case(&[1_i32, 2, 3, 4, 5], 3.0)]
    #[test_case(&[1.0, 2.0, 3.0, 4.0, 5.0], 3.0)]
    #[test_case(&[5_u32, 4, 1, 3, 2], 3.0)]
    #[test_case(&[0, u32::MAX], 2_147_483_647.5)]
    #[test_case(&[0, u32::MAX, u32::MAX], 2_863_311_530.0)]
    #[test_case(&[u32::MAX, 0, u32::MAX], 2_863_311_530.0)]
    #[test_case(&[0.0, 1.0, 2.0, 3.0, 4.0], 2.0)]
    fn test_average<'a, T>(values: impl IntoIterator<Item = &'a T>, expected: f64)
    where
        T: Copy + Into<f64> + 'a,
    {
        let average = super::average(values);
        assert!((average - expected).abs() < 0.01);
    }

    //    #[test_case(&[1.0_f64, 2.0, 3.0, 4.0, 5.0], 3.0)]
    //    #[test_case(&[0.0, f64::MAX], f64::MAX / 2.0)]
    //    #[test_case(&[0.0, f64::MAX, f64::MAX], f64::MAX)]
    //    #[test_case(&[0, 0, 0, 0, 0, 1], 0.0)]
    //    #[test_case(&[0, 0, 0, 0, 0, 0, 1], 0.0)]
    //fn test_median<'a, T>(values: impl IntoIterator<Item = impl Borrow<T>>, expected: f64)
    //where
    //T: Clone + Into<f64> + 'a,
    #[test]
    fn test_median() {
        let values: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
        let median = super::median(&values);
        assert!((median - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_buys_sells() {
        let frames = [
            json::Frame {
                events: vec![],
                participant_frames: HashMap::new(),
                timestamp: TimeDelta::minutes(0),
            },
            json::Frame {
                events: vec![],
                participant_frames: HashMap::new(),
                timestamp: TimeDelta::minutes(1),
            },
            json::Frame {
                events: vec![
                    json::Event::ItemPurchased {
                        item_id: 2503,
                        participant_id: 1,
                        timestamp: TimeDelta::minutes(1) + TimeDelta::seconds(30),
                    },
                    json::Event::ItemPurchased {
                        item_id: 2503,
                        participant_id: 2,
                        timestamp: TimeDelta::minutes(1) + TimeDelta::seconds(45),
                    },
                ],
                participant_frames: HashMap::new(),
                timestamp: TimeDelta::minutes(2),
            },
            json::Frame {
                events: vec![
                    json::Event::ItemSold {
                        item_id: 2503,
                        participant_id: 1,
                        timestamp: TimeDelta::minutes(2) + TimeDelta::seconds(25),
                    },
                    json::Event::ItemPurchased {
                        item_id: 3118,
                        participant_id: 1,
                        timestamp: TimeDelta::minutes(2) + TimeDelta::seconds(30),
                    },
                ],
                participant_frames: HashMap::new(),
                timestamp: TimeDelta::minutes(3),
            },
            json::Frame {
                events: vec![
                    json::Event::ItemPurchased {
                        item_id: 2503,
                        participant_id: 2,
                        timestamp: TimeDelta::minutes(3) + TimeDelta::seconds(30),
                    },
                    json::Event::ItemUndo {
                        before_id: 2503,
                        after_id: 0,
                        gold_gain: 1800,
                        participant_id: 2,
                        timestamp: TimeDelta::minutes(3) + TimeDelta::seconds(31),
                    },
                    json::Event::ItemPurchased {
                        item_id: 2504,
                        participant_id: 2,
                        timestamp: TimeDelta::minutes(3) + TimeDelta::seconds(32),
                    },
                    json::Event::ItemPurchased {
                        item_id: 3116,
                        participant_id: 1,
                        timestamp: TimeDelta::minutes(3) + TimeDelta::seconds(45),
                    },
                ],
                participant_frames: HashMap::new(),
                timestamp: TimeDelta::minutes(4),
            },
        ];
        let expected_player1 = vec![
            TimeDelta::minutes(2) + TimeDelta::seconds(30),
            TimeDelta::minutes(3) + TimeDelta::seconds(45),
        ];
        let expected_player2 = vec![
            TimeDelta::minutes(1) + TimeDelta::seconds(45),
            TimeDelta::minutes(3) + TimeDelta::seconds(32),
        ];
        let player1_buys = super::get_legendary_buys(&frames, 1);
        let player2_buys = super::get_legendary_buys(&frames, 2);
        assert_eq!(player1_buys, expected_player1);
        assert_eq!(player2_buys, expected_player2);
    }

    #[test_case(1001, json::ItemType::Boots)]
    #[test_case(1011, json::ItemType::Epic)]
    #[test_case(1026, json::ItemType::Basic)]
    #[test_case(3040, json::ItemType::Legendary)]
    #[test_case(3041, json::ItemType::Legendary)]
    #[test_case(3046, json::ItemType::Legendary)]
    #[test_case(2055, json::ItemType::Consumable)]
    #[test_case(2003, json::ItemType::Potion)]
    #[test_case(3340, json::ItemType::Trinket)]
    #[test_case(1054, json::ItemType::Starter)]
    fn test_ranks_map(item_id: i32, expected: json::ItemType) {
        assert_eq!(super::ITEM_RANKS.get(&item_id), Some(&expected));
    }
}
