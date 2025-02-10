use std::{
    error::Error,
    fs::{self, File, ReadDir},
    io::{self, BufRead, BufReader},
    path::Path,
};

use eframe::egui;
use xmltree::Element;

fn main() -> eframe::Result {
    let mut app = App {
        ..Default::default()
    };
    app.load_dir(Path::new(
        "/home/nathan/.local/share/Steam/steamapps/common/Noita/mods",
    ))
    .unwrap();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Noita Mod Manager",
        options,
        Box::new(|cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

enum GitHost {
    Github,
    Gitlab,
    Other,
}

struct GitMod {
    remote: Option<String>,
}

struct SteamMod {
    workshop_id: String,
}

struct ModWorkshopMod {
    link: String,
}

enum ModSource {
    Git(GitMod),
    Steam(SteamMod),
    ModWorkshop(ModWorkshopMod),
    Manual,
}

struct NormalMod {
    enabled: bool,
}

enum ModKind {
    Normal(NormalMod),
    Translation,
    Gamemode,
}

struct Mod {
    source: ModSource,
    kind: ModKind,
    name: String,
    description: String,
    unsafe_api: bool,
}

impl Mod {
    fn render(&self, ui: &mut egui::Ui) {
        ui.label(&self.name);
    }
}

struct App {
    search: String,
    mods: Vec<Mod>,
}

impl App {
    fn load_dir(&mut self, dir: &Path) -> Result<(), Box<dyn Error>> {
        for item in fs::read_dir(dir)? {
            let item = item?;
            let path = item.path();
            if !path.is_dir() {
                continue;
            }
            let mod_xml = path.join("mod.xml");
            if !mod_xml.is_file() {
                continue;
            }
            let file = File::open(mod_xml)?;
            let reader = BufReader::new(file);
            // TODO: port NXML to rust and use it here
            let tree = Element::parse(reader)?;
            fn get(tree: &Element, key: String, default: String) -> String {
                if let Some(x) = tree.attributes.get(&key) {
                    x.to_string()
                } else {
                    default
                }
            }
            let nmod = Mod {
                source: ModSource::Manual,
                kind: ModKind::Normal(NormalMod { enabled: true }),
                name: get(&tree, "name".to_owned(), "unnamed".to_owned()),
                description: get(&tree, "description".to_owned(), "".to_owned()),
                unsafe_api: get(
                    &tree,
                    "request_no_api_restrictions".to_owned(),
                    "0".to_owned(),
                ) == "0",
            };
            self.mods.push(nmod);
        }
        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        return App {
            search: "".to_owned(),
            mods: Vec::new(),
        };
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mod Manager");
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.search);
            });
            egui::SidePanel::right("right").show_inside(ui, |ui| {
                ui.label("right");
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for i in 1..=1000 {
                            ui.label("hi ".to_owned() + &i.to_string());
                        }
                    })
            });
            egui::CentralPanel::default().show_inside(ui, |ui| {
                for nmod in self.mods.iter() {
                    nmod.render(ui);
                }
            });
        });
    }
}
