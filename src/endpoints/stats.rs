use crate::{
    fetcher::{check_or_start_fetching, RedirectOrContinue},
    internal_server_error,
    riot_api::{get_puuid_and_canonical_name, json, update_match_history},
    Player, Result, State,
};
use actix_web::{get, web, Either, Responder, Result as ActixResult};
use askama_actix::Template;
use chrono::{TimeDelta, Utc};
use itertools::Itertools;
use log::{debug, info};
use ordered_float::OrderedFloat;
use std::collections::HashMap;

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

fn average_f64<'a>(values: impl IntoIterator<Item = &'a f64>) -> f64 {
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

struct StatsAtMinuteGathering {
    cs_per_minute: f64,
    gold_diff: i32,
    cs_diff: i32,
    xp_diff: i32,
}

#[derive(Debug)]
struct StatsAtMinute {
    cs_per_minute_average: f64,
    cs_per_minute_median: f64,
    gold_diff_average: f64,
    gold_diff_median: f64,
    cs_diff_average: f64,
    cs_diff_median: f64,
    xp_diff_average: f64,
    xp_diff_median: f64,
}

struct WeekStatsGathering {
    wins: u32,
    losses: u32,
    cs_per_minute: Vec<f64>,
    gold_share: Vec<f64>,
    champion_damage_share: Vec<f64>,
    objective_damage_share: Vec<f64>,
    vision_share: Vec<f64>,
    solo_kills: Vec<u32>,
    solo_deaths: Vec<u32>,
    stats_at: HashMap<u32, Vec<StatsAtMinuteGathering>>,
    positions_blue: Vec<(i64, json::Point)>,
    positions_red: Vec<(i64, json::Point)>,
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

#[derive(Debug)]
struct WeekStats {
    number: i64,
    wins: u32,
    losses: u32,
    games_played: u32,
    win_rate: f64,
    cs_per_minute_average: f64,
    cs_per_minute_median: f64,
    gold_share_average: f64,
    gold_share_median: f64,
    champion_damage_share_average: f64,
    champion_damage_share_median: f64,
    objective_damage_share_average: f64,
    objective_damage_share_median: f64,
    vision_share_average: f64,
    vision_share_median: f64,
    solo_kills_average: f64,
    solo_kills_median: f64,
    solo_deaths_average: f64,
    solo_deaths_median: f64,
    at_minute_stats: Vec<(u32, StatsAtMinute)>,
}

#[derive(Template)]
#[template(path = "stats.html")]
struct DisplayData {
    player: Player,
    weeks: Vec<WeekStats>,
}

const NUM_WEEKS: i64 = 4;

async fn calc_stats(state: State, mut player: Player) -> Result<DisplayData> {
    let from = Utc::now() - chrono::Duration::weeks(NUM_WEEKS);
    debug!("Getting puuid");
    let puuid = get_puuid_and_canonical_name(&state, &mut player).await?;
    debug!("Getting match history");
    update_match_history(&state, &player, from).await?;
    debug!("Calculating stats");
    let now = Utc::now();
    let week_stats = state
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
            WeekStatsGathering {
                wins: 0,
                losses: 0,
                cs_per_minute: vec![],
                gold_share: vec![],
                champion_damage_share: vec![],
                objective_damage_share: vec![],
                vision_share: vec![],
                solo_kills: vec![],
                solo_deaths: vec![],
                positions_blue: vec![],
                positions_red: vec![],
                stats_at: HashMap::new(),
            },
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

                let team_gold = player_team.iter().map(|p| p.gold_earned as f64).sum::<f64>();
                let gold_share = 100_f64 * player.gold_earned as f64 / team_gold;
                stats.gold_share.push(gold_share);

                let team_champion_damage = player_team.iter().map(|p| p.total_damage_dealt_to_champions as f64).sum::<f64>();
                let champion_damage_share = 100_f64 * player.total_damage_dealt_to_champions as f64 / team_champion_damage;
                stats.champion_damage_share.push(champion_damage_share);

                let team_objective_damage = player_team.iter().map(|p| p.damage_dealt_to_objectives as f64).sum::<f64>();
                let objective_damage_share = 100_f64 * player.damage_dealt_to_objectives as f64 / team_objective_damage;
                stats.objective_damage_share.push(objective_damage_share);

                let team_vision = player_team.iter().map(|p| p.vision_score as f64).sum::<f64>();
                let vision_share = 100_f64 * player.vision_score as f64 / team_vision;
                stats.vision_share.push(vision_share);

                let timeline_player_id = timeline_get_player_id(&timeline, &puuid);
                let timeline_opponent_id = timeline_get_player_id(&timeline, &opponent.puuid);
                let team_other_player_ids = player_team.iter().filter_map(|p| if p.puuid != puuid {
                    return Some(timeline_get_player_id(&timeline, &p.puuid))
                } else {
                    None
                }).collect::<Vec<_>>();

                for minute in MINUTES_AT {
                    let stats_at = frame_stats_at(
                        &timeline.info.frames,
                        timeline_player_id,
                        timeline_opponent_id,
                        TimeDelta::minutes(minute as i64),
                    );
                    if let Some(stats_at) = stats_at {
                        stats
                            .stats_at
                            .entry(minute)
                            .or_default()
                            .push(stats_at);
                    }
                }

                let solo_kills = timeline
                    .info
                    .frames
                    .iter()
                    .map(|f| {
                        f.events
                            .iter()
                            .filter(|e| {
                                if let json::Event::ChampionKill(kill) = e {
                                    kill.killer_id == timeline_player_id
                                        && kill.assisting_participant_ids.is_empty()
                                } else {
                                    false
                                }
                            })
                            .count() as u32
                    })
                    .sum::<u32>();
                stats.solo_kills.push(solo_kills);

                let deaths = timeline
                    .info
                    .frames
                    .iter()
                    .map(|f| {
                        f.events
                            .iter()
                            .filter_map(|e| {
                                if let json::Event::ChampionKill(kill) = e {
                                    if kill.victim_id == timeline_player_id {
                                        return Some((kill.timestamp, kill.position.clone()));
                                    }
                                }
                                None
                            })
                        })
                        .flatten()
                        .collect::<Vec<_>>();
                let solo_deaths = deaths.iter().filter(|(timestamp, position)| {
                    // Get the frames before and after the kill. Both should exist because there's the start and end of the game frames.
                    let frame_before = timeline.info.frames.iter().filter(|f| f.timestamp < *timestamp).last().unwrap();
                    let frame_after = timeline.info.frames.iter().find(|f| f.timestamp > *timestamp).unwrap();
                    // Check if the player was the only one in the area
                    for frame in [frame_before, frame_after] {
                        for pid in team_other_player_ids.iter() {
                            let other_player = frame.participant_frames.get(pid).unwrap();
                            if other_player.position.distance(&position) < 4000.0 {
                                return false;
                            }
                        }
                    }
                    true
                }).count();
                stats.solo_deaths.push(solo_deaths as u32);

//                    if player.team_position == "JUNGLE" {
                    let positions = match player.team_id {
                        100 => Some(&mut stats.positions_blue),
                        200 => Some(&mut stats.positions_red),
                        _ => None,
                    };
                    if let Some(positions) = positions {
                        positions.extend(timeline.info.frames.iter().filter_map(|f| {
                            let minute = f.timestamp.num_minutes();
                            if minute > 0 && minute <= 15 {
                                let mut pos = f.participant_frames.get(&timeline_player_id).unwrap().position.clone();
                                pos.x /= 37;
                                pos.y /= 37;
                                pos.y = 400 - pos.y;
                                Some((minute, pos))
                            } else {
                                None
                            }
                        }));
                    }
//                    }
                stats
            },
        );

        let cs_per_minute_average = average_f64(&week_stats.cs_per_minute);
        let cs_per_minute_median = median_f64(&week_stats.cs_per_minute);
        let gold_share_average = average_f64(&week_stats.gold_share);
        let gold_share_median = median_f64(&week_stats.gold_share);
        let champion_damage_share_average = average_f64(&week_stats.champion_damage_share);
        let champion_damage_share_median = median_f64(&week_stats.champion_damage_share);
        let objective_damage_share_average = average_f64(&week_stats.objective_damage_share);
        let objective_damage_share_median = median_f64(&week_stats.objective_damage_share);
        let vision_share_average = average_f64(&week_stats.vision_share);
        let vision_share_median = median_f64(&week_stats.vision_share);
        let solo_kills_average = average(week_stats.solo_kills.iter().copied());
        let solo_kills_median = median(week_stats.solo_kills.iter().copied());
        let solo_deaths_average = average(week_stats.solo_deaths.iter().copied());
        let solo_deaths_median = median(week_stats.solo_deaths.iter().copied());
        info!(
            "{weeks_ago} weeks ago: Wins={wins} Losses={losses} ({winrate}%) CS/min={cpm:0.1}/{cpmm:0.1} Gold Share={gs:.1}%/{gsm:.1}%",
            wins = week_stats.wins,
            losses = week_stats.losses,
            winrate = 100 * week_stats.wins / (week_stats.wins + week_stats.losses),
            cpm = cs_per_minute_average,
            cpmm = cs_per_minute_median,
            gs = gold_share_average,
            gsm = gold_share_median,
        );
        info!("    Champion Damage Share={champion_damage_share_average:.1}%/{champion_damage_share_median:.1}%");
        info!("    Objective Damage Share={objective_damage_share_average:.1}%/{objective_damage_share_median:.1}%");
        info!("    Vision Share={vision_share_average:.1}%/{vision_share_median:.1}%");
        info!("    Solo Kills={solo_kills_average:.1}/{solo_kills_median:.1} (average/median)");
        info!("    Solo Deaths={solo_deaths_average:.1}/{solo_deaths_median:.1} (average/median)");
        let at_minute_stats = MINUTES_AT.iter().filter_map(|&minute| {
            let Some(stats_at) = week_stats.stats_at.get(&minute) else {
                return None;
            };
            let cs_per_minute_avg = average(stats_at.iter().map(|s| s.cs_per_minute));
            let cs_per_minute = median_f64(stats_at.iter().map(|s| &s.cs_per_minute));
            let gold_diff_avg = average(stats_at.iter().map(|s| s.gold_diff));
            let gold_diff = median(stats_at.iter().map(|s| s.gold_diff));
            let cs_diff_avg = average(stats_at.iter().map(|s| s.cs_diff));
            let cs_diff = median(stats_at.iter().map(|s| s.cs_diff));
            let xp_diff_avg = average(stats_at.iter().map(|s| s.xp_diff));
            let xp_diff = median(stats_at.iter().map(|s| s.xp_diff));

            info!(
                "  {minute} minutes: CS/min={cpma:0.1}/{cpm:0.1} Gold diff={gda:0.0}/{gd:0.0} CS diff={csda:0.1}/{csd:0.1} (average/median)",
                minute = minute,
                cpma = cs_per_minute_avg,
                cpm = cs_per_minute,
                gda = gold_diff_avg,
                gd = gold_diff,
                csda = cs_diff_avg,
                csd = cs_diff,
            );
            Some((minute, StatsAtMinute {
                cs_per_minute_average: cs_per_minute_avg,
                cs_per_minute_median: cs_per_minute,
                gold_diff_average: gold_diff_avg,
                gold_diff_median: gold_diff,
                cs_diff_average: cs_diff_avg,
                cs_diff_median: cs_diff,
                xp_diff_average: xp_diff_avg,
                xp_diff_median: xp_diff,
            }))
        }).collect();

        // Write positions to a file
        let blue_filename = format!("positions_blue_{}.json", weeks_ago);
        let red_filename = format!("positions_red_{}.json", weeks_ago);
        let blue_file = std::fs::File::create(&blue_filename)?;
        let red_file = std::fs::File::create(&red_filename)?;
        let positions_blue = week_stats.positions_blue.into_iter().into_group_map();
        let positions_red = week_stats.positions_red.into_iter().into_group_map();
        serde_json::to_writer(blue_file, &positions_blue)?;
        serde_json::to_writer(red_file, &positions_red)?;
        info!("Wrote positions to {blue_filename} and {red_filename}");
        Ok(WeekStats {
            number: NUM_WEEKS - weeks_ago,
            wins: week_stats.wins,
            losses: week_stats.losses,
            win_rate: 100.0 * week_stats.wins as f64 / (week_stats.wins + week_stats.losses) as f64,
            games_played: week_stats.wins + week_stats.losses,
            cs_per_minute_average,
            cs_per_minute_median,
            gold_share_average,
            gold_share_median,
            champion_damage_share_average,
            champion_damage_share_median,
            objective_damage_share_average,
            objective_damage_share_median,
            vision_share_average,
            vision_share_median,
            solo_kills_average,
            solo_kills_median,
            solo_deaths_average,
            solo_deaths_median,
            at_minute_stats,
        })
    })
    .collect::<Result<Vec<_>>>()?;
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
