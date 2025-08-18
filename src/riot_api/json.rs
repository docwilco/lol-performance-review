use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{serde_as, DurationMilliSeconds, TimestampMilliSeconds};
use std::collections::HashMap;

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
    pub player0: Option<i32>,
    #[serde(rename = "playerScore1")]
    pub player1: Option<i32>,
    #[serde(rename = "playerScore2")]
    pub player2: Option<i32>,
    #[serde(rename = "playerScore3")]
    pub player3: Option<i32>,
    #[serde(rename = "playerScore4")]
    pub player4: Option<i32>,
    #[serde(rename = "playerScore5")]
    pub player5: Option<i32>,
    #[serde(rename = "playerScore6")]
    pub player6: Option<i32>,
    #[serde(rename = "playerScore7")]
    pub player7: Option<i32>,
    #[serde(rename = "playerScore8")]
    pub player8: Option<i32>,
    #[serde(rename = "playerScore9")]
    pub player9: Option<i32>,
    #[serde(rename = "playerScore10")]
    pub player10: Option<i32>,
    #[serde(rename = "playerScore11")]
    pub player11: Option<i32>,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ItemType {
    Basic,
    Boots,
    Consumable,
    Distributed,
    Epic,
    Legendary,
    Minion,
    Potion,
    Special,
    Starter,
    Trinket,
    Turret,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerkStyle {
    pub description: String,
    pub selections: Vec<PerkSelection>,
    pub style: i32,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Deserialize,
    Serialize,
    strum::EnumIter,
    strum::Display,
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
    pub assists: i32,
    pub champion_name: String,
    pub damage_dealt_to_objectives: i32,
    pub deaths: i32,
    pub gold_earned: i32,
    pub kills: i32,
    pub puuid: String,
    pub team_id: i32,
    pub team_position: Role,
    pub total_ally_jungle_minions_killed: i32,
    pub total_damage_dealt_to_champions: i32,
    pub total_enemy_jungle_minions_killed: i32,
    pub total_minions_killed: i32,
    pub vision_score: i32,
    pub win: bool,
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoShadow {
    end_of_game_result: Option<String>,
    #[serde_as(as = "TimestampMilliSeconds<i64>")]
    game_creation: DateTime<Utc>,
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
    pub end_of_game_result: Option<String>,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantFrame {
    pub total_gold: i32,
    pub current_gold: i32,
    pub minions_killed: i32,
    pub jungle_minions_killed: i32,
    pub position: Point,
    pub xp: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
    FeatUpdate,
    GameEnd {
        game_id: i64,
        #[serde_as(as = "TimestampMilliSeconds<i64>")]
        real_timestamp: DateTime<Utc>,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
        winning_team: i32,
    },
    ItemDestroyed {
        item_id: i32,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemPurchased {
        item_id: i32,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemSold {
        item_id: i32,
        participant_id: usize,
        #[serde_as(as = "DurationMilliSeconds<i64>")]
        timestamp: TimeDelta,
    },
    ItemUndo {
        after_id: i32,
        before_id: i32,
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
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
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
