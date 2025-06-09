use serde::Deserialize;
use std::{
    fs::File,
    io::{BufReader, Read},
    marker::PhantomData,
    path::Path,
};
use toml;

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[derive(Deserialize)]
struct Config {
    save00_path: String,
    mods_path: String,
    workshop_path: String,
}

mod app;
mod collapsing_ui;
mod ext;
mod icons;
mod r#mod;
use anyhow::Context;
use app::{App, ProfilerInfo};
use r#mod::Mod;

fn main() -> anyhow::Result<()> {
    let mut content_str = String::new();
    let _ = &BufReader::new(
        File::open(Path::new("./Config.toml").to_path_buf()).context("Reading config file")?,
    )
    .read_to_string(&mut content_str)
    .context("Reading config to string")?;
    let config: Config = toml::from_str(&content_str).context("Parsing config")?;
    let mod_config = Path::new(&config.save00_path).join("mod_config.xml");
    let mod_settings = Path::new(&config.save00_path).join("mod_settings.bin");
    let mods_dir = Path::new(&config.mods_path);
    let workshop_dir = Path::new(&config.workshop_path);
    #[cfg(feature = "profiler")]
    let profiler = ProfilerInfo {
        frame_counter: 0,
        profiler: pprof::ProfilerGuardBuilder::default()
            .frequency(1000)
            .blocklist(&["libc", "libgcc", "pthread", "vdso"])
            .build()
            .unwrap(),
    };
    #[cfg(not(feature = "profiler"))]
    let profiler = ProfilerInfo {
        profiler: PhantomData,
    };
    let app = App::new(
        &mod_config,
        Some(workshop_dir),
        Some(mods_dir),
        &mod_settings,
        profiler,
    )
    .context("Creating app")?;

    app.run().context("Running app")
}
