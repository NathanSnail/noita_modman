use std::{
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
};

mod conditional;
use anyhow::{anyhow, bail, Context};
use conditional::Condition;
use eframe::egui;
use egui::{
    ahash::{HashSet, HashSetExt},
    Color32, FontId, RichText,
};
use xmltree::Element;

const STEAM: char = '\u{E623}';
const TRANSLATION: char = '\u{1F4D5}';
const GAMEMODE: char = '\u{1F30F}';
const NORMAL: char = '\u{1F5A5}';
const UNSAFE: char = '\u{26A0}';

fn main() -> anyhow::Result<()> {
    let mod_config = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_config.xml");
    let mut app = App::new(&mod_config);
    let enabled_set = App::parse_enabled(BufReader::new(
        File::open(&mod_config).with_context(|| "Opening mod config")?,
    ))?;

    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/common/Noita/mods"),
        &enabled_set,
        false,
    )?;
    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/workshop/content/881100"),
        &enabled_set,
        true,
    )?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Noita Mod Manager",
        options,
        Box::new(|cc| {
            // This gives us image support:
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
    /// this is just needed for saving as we loaded it
    settings_fold_open: bool,
}

impl Mod {
    fn matches(&self, conditions: &[Condition]) -> bool {
        conditions
            .iter()
            .map(|x| x.matches(&self))
            .reduce(|a, b| a && b)
            .unwrap_or(true)
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        let mut done_checkbox = false;
        match &mut self.kind {
            ModKind::Normal(normal_mod) => {
                ui.checkbox(&mut normal_mod.enabled, "")
                    .on_hover_text("Enabled");
                done_checkbox = true;
            }
            _ => {}
        }
        if !done_checkbox {
            ui.horizontal(|_| {});
        }
        let mut done_source = false;
        match &self.source {
            ModSource::Git(git_mod) => {
                let remote_url = git_mod.remote.clone();
                use egui::special_emojis::GIT;
                use egui::special_emojis::GITHUB;
                if let Some(url) = remote_url {
                    ui.hyperlink_to(
                        match git_mod.host {
                            GitHost::Github => format!("{GITHUB}"),
                            GitHost::Gitlab => format!("{GIT}"),
                            GitHost::Other => format!("{GIT}"),
                        },
                        &url,
                    )
                    .on_hover_text(match &git_mod.host {
                        GitHost::Github => format!("Github ({url})"),
                        GitHost::Gitlab => format!("Gitlab ({url})"),
                        GitHost::Other => format!("Unkown remote ({url})"),
                    });
                    done_source = true;
                }
            }
            ModSource::Steam(steam_mod) => {
                let steam_url = "https://steamcommunity.com/sharedfiles/filedetails/?id="
                    .to_owned()
                    + &steam_mod.workshop_id;
                ui.hyperlink_to(format!("{STEAM}"), &steam_url)
                    .on_hover_text(format!("Steam ({steam_url})"));
                done_source = true;
            }
            _ => {}
        }
        if !done_source {
            // to manipulate the grid
            ui.horizontal(|_| {});
        }
        ui.horizontal(|ui| {
            ui.label(
                match &self.kind {
                    ModKind::Normal(_) => NORMAL,
                    ModKind::Translation => TRANSLATION,
                    ModKind::Gamemode => GAMEMODE,
                }
                .to_string(),
            )
            .on_hover_text(match &self.kind {
                ModKind::Normal(_) => "Normal mod",
                ModKind::Translation => "Translation mod",
                ModKind::Gamemode => "Gamemode mod",
            });
            if self.unsafe_api {
                ui.label(RichText::new(format!("{UNSAFE}")).color(Color32::from_rgb(255, 220, 40)))
                    .on_hover_text("Unsafe mod");
            }
        });
        ui.label(&self.name).on_hover_text(
            "(".to_owned()
                + &self.id
                + if let ModSource::Steam(_) = &self.source {
                    // hax to fix borrow stuff
                    " - "
                } else {
                    ""
                }
                + if let ModSource::Steam(steam_mod) = &self.source {
                    &steam_mod.workshop_id
                } else {
                    ""
                }
                + if &self.description != "" {
                    ")\n\n"
                } else {
                    ")"
                }
                + &self.description,
        );
    }
}

struct App<'a> {
    search: String,
    mod_config: &'a Path,
    mods: Vec<Mod>,
}

impl App<'_> {
    fn parse_enabled<R>(src: R) -> anyhow::Result<HashSet<String>>
    where
        R: Read,
    {
        let tree = Element::parse(src).with_context(|| "Parsing mod config failed")?;
        let mut set = HashSet::new();
        for child in tree.children.iter() {
            let element = child.as_element().map(|x| Ok(x)).unwrap_or(Err(anyhow!(
                "Couldn't convert xmlnode to element? While parsing mod config"
            )))?;
            if element
                .attributes
                .get("enabled")
                .map(|x| Ok(x))
                .unwrap_or(Err(anyhow!("Mod config broken, missing enabled")))?
                == "1"
            {
                set.insert(
                    element
                        .attributes
                        .get("name")
                        .map(|x| Ok(x))
                        .unwrap_or(Err(anyhow!("Mod config broken, missing name")))?
                        .clone(),
                );
            }
        }
        Ok(set)
    }

    fn load_dir(
        &mut self,
        dir: &Path,
        enabled_mods: &HashSet<String>,
        is_workshop: bool,
    ) -> anyhow::Result<()> {
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
                bail!("Path doesn't have a filename???")
            };
            let mut id = suffix.clone();

            let source = if is_workshop {
                id = "".to_owned();
                File::open(path.join("mod_id.txt"))
                    .with_context(|| format!("Opening mod_id.txt for {suffix}"))?
                    .read_to_string(&mut id)
                    .with_context(|| format!("Reading mod_id.txt for {suffix}"))?;
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

            let enabled = enabled_mods.contains(&id);

            let nmod = Mod {
                source,
                id,
                kind: if get(&tree, "is_translation".to_owned(), "0".to_owned()) == "1" {
                    ModKind::Translation
                } else if get(&tree, "is_game_mode".to_owned(), "0".to_owned()) == "1" {
                    ModKind::Gamemode
                } else {
                    ModKind::Normal(NormalMod { enabled })
                },
                settings_fold_open: get(&tree, "settings_fold_open".to_string(), "0".to_owned())
                    == "1",
                name: get(&tree, "name".to_owned(), "unnamed".to_owned()),
                description: get(&tree, "description".to_owned(), "".to_owned())
                    .replace("\\n", "\n"),
                unsafe_api: get(
                    &tree,
                    "request_no_api_restrictions".to_owned(),
                    "0".to_owned(),
                ) == "1",
            };
            self.mods.push(nmod);
        }
        Ok(())
    }
}

impl App<'_> {
    pub fn new<'a>(mod_config: &'a Path) -> App<'a> {
        return App {
            mod_config,
            search: "".to_owned(),
            mods: Vec::new(),
        };
    }
}

impl eframe::App for App<'_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mod Manager");
            if ui.button("Save").on_hover_text("Save mod config for use in game (requires restarting Noita)").clicked() {
                let buf = "<Mods>\n".to_string()
                    + &self
                        .mods
                        .iter()
                        .map(|x| {
                            let id = &x.id;
                            let enabled = if let ModKind::Normal(normal_mod) = &x.kind {
                                normal_mod.enabled as usize
                            } else {
                                0
                            };
                            let workshop_item_id = if let ModSource::Steam(steam_mod) = &x.source {
                                &steam_mod.workshop_id
                            } else {
                                "0"
                        };
                            let settings_fold_open = x.settings_fold_open as usize;
                            format!("\t<Mod enabled=\"{enabled}\" name=\"{id}\" settings_fold_open=\"{settings_fold_open}\" workshop_item_id=\"{workshop_item_id}\" />\n")
                        })
                        .reduce(|a, b| a + &b).unwrap_or("".to_owned()) + "</Mods>";
                let mut file = File::create(self.mod_config)
                    .with_context(|| "Opening mod config for saving")
                    .unwrap();
                write!(file, "{}", buf).with_context(|| "Writing to mod config").unwrap();
                file.flush().with_context(|| "Flushing file").unwrap();
            }
            let cur_search = self.search.clone();
            let conditions_err: Vec<_> = cur_search
                .split(" ")
                .map(|x| (x, Condition::new(x)))
                .filter(|x| x.0 != "")
                .collect();
            let broken_terms: &Vec<_> = &conditions_err
                .iter()
                .filter(|x| x.1.is_none())
                .map(|x| x.0)
                .collect();
            let conditions: &Vec<_> = &conditions_err
                .iter()
                .filter(|x| x.1.is_some())
                .map(|x| x.1.clone().unwrap())
                .collect();
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.search)
                    .on_hover_text(Condition::special_terms());
                if !broken_terms.is_empty() {
                    ui.label("Broken search terms: ");
                    broken_terms.iter().for_each(|x| {
                        ui.label(x.to_string());
                    });
                }
            });

            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Grid::new("mod_grid").striped(true).show(ui, |ui| {
                        for nmod in self.mods.iter_mut().filter(|x| x.matches(&conditions)) {
                            nmod.render(ui);
                            ui.end_row();
                        }
                    });
                });
        });
    }
}
