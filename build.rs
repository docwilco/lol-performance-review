use cached::proc_macro::io_cached;
use itertools::Itertools;
use serde_json::Value;
use std::{
    env,
    error::Error,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

#[io_cached(
    disk = true,
    disk_dir = "cache",
    key = "u8",
    convert = r#"{ 0 }"#,
    time = 43200, // 12 hours
    map_error = r##"|e| e"##
)]
fn fetch_champs() -> Result<Value, Box<dyn Error>> {
    // Fetch
    // https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-summary.json
    // and make a PHF out of it for normalized name to display name.
    let url = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-summary.json";
    let champs: Value = reqwest::blocking::get(url)?.json()?;
    Ok(champs)
}

fn main() -> Result<(), Box<dyn Error>> {
    let champs = fetch_champs()?;
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
    Ok(())
}
