use std::path::Path;

mod app;
mod ext;
mod r#mod;
use anyhow::{anyhow, Context};
use app::App;
use eframe::egui;
use egui::FontId;
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Noita Mod Manager",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.style_mut(|style| {
                style.text_styles.insert(
                    egui::TextStyle::Body,
                    FontId::new(20.0, egui::FontFamily::Proportional),
                );
            });
            Ok(Box::new(app))
        }),
    );
    result.map_err(|x| anyhow!(format!("{x:?}")))
}
