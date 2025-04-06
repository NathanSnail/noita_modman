use std::{marker::PhantomData, path::Path};

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

mod app;
mod collapsing_ui;
mod ext;
mod icons;
mod r#mod;
use anyhow::Context;
use app::{App, ProfilerInfo};
use r#mod::Mod;

fn main() -> anyhow::Result<()> {
    let mod_config = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_config.xml");
    let mod_settings = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_settings.bin");
    // let mod_settings = Path::new("./saved_settings");
    let mods_dir = Path::new("/home/nathan/.local/share/Steam/steamapps/common/Noita/mods");
    let workshop_dir =
        Path::new("/home/nathan/.local/share/Steam/steamapps/workshop/content/881100");
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
        mod_settings,
        profiler,
    )
    .context("Creating app")?;

    app.run().context("Running app")
}
