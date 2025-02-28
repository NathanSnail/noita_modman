use std::path::Path;

mod app;
mod ext;
mod icons;
mod r#mod;
use anyhow::Context;
use app::App;
use r#mod::Mod;

fn main() -> anyhow::Result<()> {
    let mod_config = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_config.xml");
    let mod_settings = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_settings.bin");
    // let mod_settings = Path::new("./saved_settings");
    let mods_dir = Path::new("/home/nathan/.local/share/Steam/steamapps/common/Noita/mods");
    let workshop_dir =
        Path::new("/home/nathan/.local/share/Steam/steamapps/workshop/content/881100");

    let app = App::new(
        &mod_config,
        Some(workshop_dir),
        Some(mods_dir),
        mod_settings,
    )
    .context("Creating app")?;

    app.run().context("Running app")
}
