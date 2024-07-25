use std::collections::HashMap;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DurationMilliSeconds, TimestampMilliSeconds};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub data_version: String,
    pub match_id: String,
    pub participants: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub bans: Vec<Ban>,
    pub objectives: Objectives,
    #[serde(rename = "teamId")]
    pub id: i32,
    pub win: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ban {
    pub champion_id: i32,
    pub pick_turn: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Objectives {
    pub baron: Objective,
    pub champion: Objective,
    pub dragon: Objective,
    pub horde: Objective,
    pub inhibitor: Objective,
    pub rift_herald: Objective,
    pub tower: Objective,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Objective {
    pub first: bool,
    pub kills: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Missions {
    #[serde(rename = "playerScore0")]
    pub player0: i32,
    #[serde(rename = "playerScore1")]
    pub player1: i32,
    #[serde(rename = "playerScore2")]
    pub player2: i32,
    #[serde(rename = "playerScore3")]
    pub player3: i32,
    #[serde(rename = "playerScore4")]
    pub player4: i32,
    #[serde(rename = "playerScore5")]
    pub player5: i32,
    #[serde(rename = "playerScore6")]
    pub player6: i32,
    #[serde(rename = "playerScore7")]
    pub player7: i32,
    #[serde(rename = "playerScore8")]
    pub player8: i32,
    #[serde(rename = "playerScore9")]
    pub player9: i32,
    #[serde(rename = "playerScore10")]
    pub player10: i32,
    #[serde(rename = "playerScore11")]
    pub player11: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Perks {
    pub stat_perks: PerkStats,
    pub styles: Vec<PerkStyle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkStats {
    pub defense: i32,
    pub flex: i32,
    pub offense: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkSelection {
    pub perk: i32,
    pub var1: i32,
    pub var2: i32,
    pub var3: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkStyle {
    pub description: String,
    pub selections: Vec<PerkSelection>,
    pub style: i32,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize, strum::EnumIter, strum::Display,
)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[serde(alias = "TOP")]
    Top,
    #[serde(alias = "JUNGLE")]
    Jungle,
    #[serde(alias = "MIDDLE")]
    Middle,
    #[serde(alias = "BOTTOM")]
    Bottom,
    #[serde(alias = "UTILITY")]
    Support,
    #[serde(alias = "")]
    None,
}

impl Role {
    pub fn lowercase(self) -> String {
        self.to_string().to_lowercase()
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde_as]
pub struct Participant {
    pub all_in_pings: i32,
    pub assist_me_pings: i32,
    pub assists: i32,
    pub baron_kills: i32,
    pub bounty_level: i32,
    pub champ_experience: i32,
    pub champ_level: i32,
    pub champion_id: i32,
    pub champion_name: String,
    pub command_pings: i32,
    pub champion_transform: i32,
    pub consumables_purchased: i32,
    pub damage_dealt_to_buildings: i32,
    pub damage_dealt_to_objectives: i32,
    pub damage_dealt_to_turrets: i32,
    pub damage_self_mitigated: i32,
    pub deaths: i32,
    pub detector_wards_placed: i32,
    pub double_kills: i32,
    pub dragon_kills: i32,
    pub eligeble_for_progression: Option<bool>,
    pub enemy_missing_pings: i32,
    pub enemy_vision_pings: i32,
    pub first_blood_assist: bool,
    pub first_blood_kill: bool,
    pub first_tower_assist: bool,
    pub first_tower_kill: bool,
    pub game_ended_in_early_surrender: bool,
    pub game_ended_in_surrender: bool,
    pub hold_pings: i32,
    pub get_back_pings: i32,
    pub gold_earned: i32,
    pub gold_spent: i32,
    pub individual_position: String,
    pub inhibitor_kills: i32,
    pub inhibitor_takedowns: i32,
    pub inhibitors_lost: i32,
    pub item0: i32,
    pub item1: i32,
    pub item2: i32,
    pub item3: i32,
    pub item4: i32,
    pub item5: i32,
    pub item6: i32,
    pub items_purchased: i32,
    pub killing_sprees: i32,
    pub kills: i32,
    pub lane: String,
    pub largest_critical_strike: i32,
    pub largest_killing_spree: i32,
    pub largest_multi_kill: i32,
    pub longest_time_spent_living: i32,
    pub magic_damage_dealt: i32,
    pub magic_damage_dealt_to_champions: i32,
    pub magic_damage_taken: i32,
    pub missions: Missions,
    pub neutral_minions_killed: i32,
    pub need_vision_pings: i32,
    pub nexus_kills: i32,
    pub objectives_stolen: i32,
    pub objectives_stolen_assists: i32,
    pub on_my_way_pings: i32,
    #[serde(rename = "participantId")]
    pub id: i32,
    pub penta_kills: i32,
    pub perks: Perks,
    pub physical_damage_dealt: i32,
    pub physical_damage_dealt_to_champions: i32,
    pub physical_damage_taken: i32,
    pub placement: i32,
    pub player_augment1: i32,
    pub player_augment2: i32,
    pub player_augment3: i32,
    pub player_augment4: i32,
    pub player_subteam_id: i32,
    pub push_pings: i32,
    pub profile_icon: i32,
    pub puuid: String,
    pub quadra_kills: i32,
    pub riot_id_game_name: String,
    pub riot_id_tagline: String,
    pub role: String,
    pub sight_wards_bought_in_game: i32,
    pub spell1_casts: i32,
    pub spell2_casts: i32,
    pub spell3_casts: i32,
    pub spell4_casts: i32,
    pub subteam_placement: i32,
    pub summoner1_casts: i32,
    pub summoner1_id: i32,
    pub summoner2_casts: i32,
    pub summoner2_id: i32,
    pub summoner_id: String,
    pub summoner_level: i32,
    pub summoner_name: String,
    pub team_early_surrendered: bool,
    pub team_id: i32,
    pub team_position: Role,
    pub time_c_cing_others: i32,
    pub time_played: i32,
    pub total_ally_jungle_minions_killed: i32,
    pub total_damage_dealt: i32,
    pub total_damage_dealt_to_champions: i32,
    pub total_damage_shielded_on_teammates: i32,
    pub total_damage_taken: i32,
    pub total_enemy_jungle_minions_killed: i32,
    pub total_heal: i32,
    pub total_heals_on_teammates: i32,
    pub total_minions_killed: i32,
    pub total_time_c_c_dealt: i32,
    pub total_time_spent_dead: i32,
    pub total_units_healed: i32,
    pub triple_kills: i32,
    pub true_damage_dealt: i32,
    pub true_damage_dealt_to_champions: i32,
    pub true_damage_taken: i32,
    pub turret_kills: i32,
    pub turret_takedowns: i32,
    pub turrets_lost: i32,
    pub unreal_kills: i32,
    pub vision_score: i32,
    pub vision_cleared_pings: i32,
    pub vision_wards_bought_in_game: i32,
    pub wards_killed: i32,
    pub wards_placed: i32,
    pub win: bool,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoShadow {
    end_of_game_result: String,
    #[serde_as(as = "TimestampMilliSeconds<i64>")]
    pub game_creation: DateTime<Utc>,
    game_duration: i64,
    #[serde_as(as = "Option<TimestampMilliSeconds<i64>>")]
    game_end_timestamp: Option<DateTime<Utc>>,
    game_id: i64,
    game_mode: String,
    game_name: String,
    #[serde_as(as = "TimestampMilliSeconds<i64>")]
    game_start_timestamp: DateTime<Utc>,
    game_type: String,
    game_version: String,
    map_id: i32,
    pub participants: Vec<Participant>,
    platform_id: String,
    queue_id: i32,
    teams: Vec<Team>,
    tournament_code: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Info {
    pub end_of_game_result: String,
    pub game_creation: DateTime<Utc>,
    pub game_duration: TimeDelta,
    pub game_end_timestamp: DateTime<Utc>,
    pub game_id: i64,
    pub game_mode: String,
    pub game_name: String,
    pub game_start_timestamp: DateTime<Utc>,
    pub game_type: String,
    pub game_version: String,
    pub map_id: i32,
    pub participants: Vec<Participant>,
    pub platform_id: String,
    pub queue_id: i32,
    pub teams: Vec<Team>,
    pub tournament_code: Option<String>,
}

impl From<InfoShadow> for Info {
    fn from(info: InfoShadow) -> Self {
        let game_duration = if info.game_end_timestamp.is_some() {
            TimeDelta::seconds(info.game_duration)
        } else {
            TimeDelta::milliseconds(info.game_duration)
        };
        let game_end_timestamp = info
            .game_end_timestamp
            .unwrap_or_else(|| info.game_start_timestamp + game_duration);
        Self {
            end_of_game_result: info.end_of_game_result,
            game_creation: info.game_creation,
            game_duration,
            game_end_timestamp,
            game_id: info.game_id,
            game_mode: info.game_mode,
            game_name: info.game_name,
            game_start_timestamp: info.game_start_timestamp,
            game_type: info.game_type,
            game_version: info.game_version,
            map_id: info.map_id,
            participants: info.participants,
            platform_id: info.platform_id,
            queue_id: info.queue_id,
            teams: info.teams,
            tournament_code: info.tournament_code,
        }
    }
}

impl From<&Info> for InfoShadow {
    fn from(info: &Info) -> Self {
        Self {
            end_of_game_result: info.end_of_game_result.clone(),
            game_creation: info.game_creation,
            game_duration: info.game_duration.num_seconds(),
            game_end_timestamp: Some(info.game_end_timestamp),
            game_id: info.game_id,
            game_mode: info.game_mode.clone(),
            game_name: info.game_name.clone(),
            game_start_timestamp: info.game_start_timestamp,
            game_type: info.game_type.clone(),
            game_version: info.game_version.clone(),
            map_id: info.map_id,
            participants: info.participants.clone(),
            platform_id: info.platform_id.clone(),
            queue_id: info.queue_id,
            teams: info.teams.clone(),
            tournament_code: info.tournament_code.clone(),
        }
    }
}

impl<'de> Deserialize<'de> for Info {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        InfoShadow::deserialize(deserializer).map(Self::from)
    }
}

impl Serialize for Info {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        InfoShadow::from(self).serialize(serializer)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub metadata: Metadata,
    pub info: Info,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantFrame {
    pub total_gold: i32,
    pub current_gold: i32,
    pub minions_killed: i32,
    pub position: Point,
    pub xp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn distance(self, other: Self) -> f64 {
        // X and Y should be 16000 at most, and 2*(16K^2) = 512M, which fits in i32
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        f64::from(dx * dx + dy * dy).sqrt()
    }
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Damage {
    pub basic: bool,
    pub magic_damage: i32,
    pub name: String,
    pub participant_id: usize,
    pub physical_damage: i32,
    pub spell_name: String,
    pub spell_slot: i32,
    pub true_damage: i32,
    #[serde(rename = "type")]
    pub damage_type: String,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionKill {
    #[serde(default)]
    pub assisting_participant_ids: Vec<usize>,
    pub bounty: i32,
    pub kill_streak_length: i32,
    pub killer_id: usize,
    pub position: Point,
    pub shutdown_bounty: i32,
    #[serde_as(as = "DurationMilliSeconds<i64>")]
    pub timestamp: TimeDelta,
    #[serde(default)]
    pub victim_damage_dealt: Vec<Damage>,
    pub victim_damage_received: Vec<Damage>,
    pub victim_id: usize,
}

#[allow(clippy::enum_variant_names)]
#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Event {
    AscendedEvent {
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    BuildingKill {
        #[serde(default)]
        assisting_participant_ids: Vec<usize>,
        bounty: i32,
        building_type: String,
        killer_id: usize,
        lane_type: String,
        position: Point,
        team_id: i32,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        tower_type: Option<String>,
    },
    CapturePoint,
    ChampionKill(ChampionKill),
    ChampionSpecialKill,
    ChampionTransform,
    DragonSoulGiven,
    EliteMonsterKill {
        #[serde(default)]
        assisting_participant_ids: Vec<usize>,
        bounty: i32,
        killer_id: usize,
        killer_team_id: i32,
        monster_sub_type: Option<String>,
        monster_type: String,
        position: Point,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    GameEnd {
        game_id: i64,
        #[serde_as(as = "TimestampMilliSeconds<i64>")]
        real_timestamp: DateTime<Utc>,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        winning_team: i32,
    },
    ItemDestroyed {
        item_id: usize,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemPurchased {
        item_id: usize,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemSold {
        item_id: usize,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemUndo {
        after_id: usize,
        before_id: usize,
        gold_gain: i32,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    LevelUp {
        level: i8,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ObjectiveBountyFinish,
    ObjectiveBountyPrestart {
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        actual_start_time: TimeDelta,
        team_id: i32,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    PauseEnd {
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        #[serde_as(as = "TimestampMilliSeconds<i64>")]
        real_timestamp: DateTime<Utc>,
    },
    PoroKingSummon,
    SkillLevelUp {
        participant_id: usize,
        skill_slot: usize,
        level_up_type: String,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    TurretPlateDestroyed {
        #[serde(default)]
        assisting_participant_ids: Vec<usize>,
        lane_type: String,
        position: Point,
        team_id: i32,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    WardKill {
        killer_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        ward_type: String,
    },
    WardPlaced {
        creator_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        ward_type: String,
    },
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub events: Vec<Event>,
    pub participant_frames: HashMap<usize, ParticipantFrame>,
    #[serde_as(as = "DurationMilliSeconds<i64>")]
    pub timestamp: TimeDelta,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineParticipant {
    pub participant_id: usize,
    pub puuid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineInfo {
    pub frames: Vec<Frame>,
    pub participants: Vec<TimelineParticipant>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub metadata: Metadata,
    pub info: TimelineInfo,
}
