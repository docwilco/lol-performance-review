use cached::proc_macro::io_cached;
use derive_more::From;
use itertools::Itertools;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    str::FromStr,
};

#[derive(Debug, From)]
pub enum Error {
    #[from]
    SerdeJson(serde_json::Error),
    #[from]
    Reqwest(reqwest::Error),
    #[from]
    DiskCache(cached::DiskCacheError),
    #[from]
    Io(std::io::Error),
    #[from]
    ParseInt(std::num::ParseIntError),
}
pub type Result<T> = core::result::Result<T, Error>;

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}

#[io_cached(
    disk = true,
    disk_dir = "cache",
    key = "String",
    convert = r#"{ url.to_string() }"#,
    map_error = r##"|e| e"##,
    time = 43200, // 12 hours
)]
fn fetch_text(url: &str) -> Result<String> {
    Ok(reqwest::blocking::get(url)?.text()?)
}

fn fetch_json<T>(url: &str) -> Result<T>
where
    T: DeserializeOwned + FromStr,
    Error: std::convert::From<<T as std::str::FromStr>::Err>,
{
    let text = fetch_text(url)?;
    Ok(T::from_str(text.as_str())?)
}

fn main() -> Result<()> {
    let champs: Value = fetch_json("https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-summary.json")?;
    let mut builder = phf_codegen::Map::new();
    let names = champs
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .sorted()
        .dedup()
        .collect::<Vec<_>>();
    for name in names {
        let normalized = name
            .chars()
            .filter_map(|c| {
                if c.is_ascii_alphabetic() {
                    Some(c.to_lowercase())
                } else {
                    None
                }
            })
            .flatten()
            .collect::<String>();
        builder.entry(normalized, &format!("\"{name}\""));
    }
    let map = builder.build();
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen-champ-names.rs");
    let mut file = BufWriter::new(File::create(path)?);
    // Pedantic does fire on generated code that's not a proc-macro.
    writeln!(&mut file, "#[allow(clippy::unreadable_literal)]")?;
    writeln!(
        &mut file,
        "static CHAMP_NAMES: phf::Map<&'static str, &'static str> = {map};"
    )?;

    let items: Value =
        fetch_json("http://cdn.merakianalytics.com/riot/lol/resources/latest/en-US/items.json")?;
    let mut builder = phf_codegen::Map::new();
    let ranks = items
        .as_object()
        .unwrap()
        .iter()
        .map(|(item_id, info)| {
            // Gangplank upgrades are items, but no rank
            if let Some(rank) = info["rank"][0].as_str() {
                // Capitalize the rank.
                let mut rankchars = rank.chars();
                let rank = rankchars
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .chain(rankchars.map(|c| c.to_ascii_lowercase()))
                    .collect::<String>();
                let item_id = item_id.parse::<i32>()?;
                return Ok(Some((item_id, rank)));
            }
            Ok(None)
        })
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>>>()?;
    for (item_id, rank) in ranks {
        builder.entry(item_id, &format!("json::ItemType::{rank}"));
    }
    let map = builder.build();
    let path = Path::new(&env::var("OUT_DIR").unwrap()).join("codegen-item-ranks.rs");
    let mut file = BufWriter::new(File::create(path)?);
    // Pedantic does fire on generated code that's not a proc-macro.
    writeln!(&mut file, "#[allow(clippy::unreadable_literal)]")?;
    writeln!(
        &mut file,
        "static ITEM_RANKS: phf::Map<i32, json::ItemType> = {map};"
    )?;
    Ok(())
}
