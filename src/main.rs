use std::{
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
};

use anyhow::{bail, Context};
use eframe::egui;
use xmltree::Element;

const STEAM: char = '\u{E623}';

fn main() -> eframe::Result {
    let mut app = App {
        ..Default::default()
    };
    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/common/Noita/mods"),
        false,
    )
    .unwrap();
    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/workshop/content/881100"),
        true,
    )
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
    host: GitHost,
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
    id: String,
    description: String,
    unsafe_api: bool,
}

impl Mod {
    fn render(&self, ui: &mut egui::Ui) {
        let mut done_source = false;
        match &self.source {
            ModSource::Git(git_mod) => {
                let remote_url = git_mod.remote.clone();
                use egui::special_emojis::GIT;
                use egui::special_emojis::GITHUB;
                if let Some(url) = remote_url {
                    ui.hyperlink_to(
                        match git_mod.host {
                            GitHost::Github => format!("{GITHUB} Github"),
                            GitHost::Gitlab => format!("{GIT} Gitlab"),
                            GitHost::Other => format!("{GIT} Remote"),
                        },
                        url,
                    );
                    done_source = true;
                }
            }
            ModSource::Steam(steam_mod) => {
                ui.hyperlink_to(
                    format!("{STEAM} Steam"),
                    "https://steamcommunity.com/sharedfiles/filedetails/?id=".to_owned()
                        + &steam_mod.workshop_id.clone(),
                );
                done_source = true;
            }
            _ => {}
        }
        if !done_source {
            // to manipulate the grid
            ui.label("");
        }
        ui.label(&self.name).on_hover_text(&self.description);
    }
}

struct App {
    search: String,
    mods: Vec<Mod>,
}

impl App {
    fn load_dir(&mut self, dir: &Path, is_workshop: bool) -> anyhow::Result<()> {
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

            let suffix = if let Some(x) = path.file_name() {
                x.to_string_lossy().to_string()
            } else {
                bail!("Item doesn't have a filename")
            };
            let mut id = suffix.clone();

            let source = if is_workshop {
                File::open(path.join("mod_id.txt"))
                    .with_context(|| format!("Workshop item {suffix} doesn't have a mod id"))?
                    .read_to_string(&mut id)
                    .with_context(|| format!("Reading mod id for {suffix} failed"))?;
                ModSource::Steam(SteamMod {
                    workshop_id: suffix.clone(),
                })
            } else if path.join(".git").is_dir() {
                let repo = git2::Repository::discover(path)?;
                let remotes = repo.remotes()?;
                let remote = repo
                    .find_remote("origin")
                    .ok()
                    .map(|x| x.url().map(|x| x.to_owned()))
                    .flatten()
                    .or(remotes.get(0).map(|x| x.to_owned()));
                let host = if let Some(url) = &remote {
                    if url.contains("github") {
                        GitHost::Github
                    } else if url.contains("gitlab") {
                        GitHost::Gitlab
                    } else {
                        GitHost::Other
                    }
                } else {
                    GitHost::Other
                };
                ModSource::Git(GitMod { remote, host })
            } else {
                ModSource::Manual
            };

            let nmod = Mod {
                source,
                id,
                kind: ModKind::Normal(NormalMod { enabled: true }),
                name: get(&tree, "name".to_owned(), "unnamed".to_owned()),
                description: get(&tree, "description".to_owned(), "".to_owned())
                    .replace("\\n", "\n"),
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
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Grid::new("mod_grid").striped(true).show(ui, |ui| {
                        for nmod in self.mods.iter() {
                            nmod.render(ui);
                            ui.end_row();
                        }
                    });
                });
        });
    }
}
