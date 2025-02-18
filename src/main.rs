use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
};

mod conditional;
use anyhow::{anyhow, bail, Context};
use conditional::Condition;
use eframe::{egui, Frame};
use egui::{vec2, Color32, FontId, Id, RichText, Sense, Window};
use xmltree::{Element, XMLNode};

const STEAM: char = '\u{E623}';
const TRANSLATION: char = '\u{1F4D5}';
const GAMEMODE: char = '\u{1F30F}';
const NORMAL: char = '\u{1F5A5}';
const UNSAFE: char = '\u{26A0}';

fn main() -> anyhow::Result<()> {
    let mod_config = Path::new("/home/nathan/.local/share/Steam/steamapps/compatdata/881100/pfx/drive_c/users/steamuser/AppData/LocalLow/Nolla_Games_Noita/save00/mod_config.xml");
    let mut app = App::new(&mod_config);

    let mod_config = App::parse_config(BufReader::new(
        File::open(&mod_config).context("Opening mod config")?,
    ))
    .context("Parsing mod config")?;
    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/common/Noita/mods"),
        false,
    )
    .context("Loading main mods folder")?;
    app.load_dir(
        Path::new("/home/nathan/.local/share/Steam/steamapps/workshop/content/881100"),
        true,
    )
    .context("Loading wokshop mods folder")?;
    app.sort_mods(&mod_config).context("Sorting mods list")?;

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

#[derive(Copy, Clone, Debug)]
enum GitHost {
    Github,
    Gitlab,
    Other,
}

#[derive(Clone, Debug)]
struct GitMod {
    remote: Option<String>,
    host: GitHost,
}

#[derive(Clone, Debug)]
struct SteamMod {
    workshop_id: String,
}

#[derive(Clone, Debug)]
struct ModWorkshopMod {
    link: String,
}

#[derive(Clone, Debug)]
enum ModSource {
    Git(GitMod),
    Steam(SteamMod),
    ModWorkshop(ModWorkshopMod),
    Manual,
}

#[derive(Copy, Clone, Debug)]
struct NormalMod {
    enabled: bool,
}

#[derive(Copy, Clone, Debug)]
enum ModKind {
    Normal(NormalMod),
    Translation,
    Gamemode,
}

#[derive(Clone, Debug)]
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

    // returns true if we got dragged
    fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut done_checkbox = false;

            let cursor_checkbox_start = ui.cursor().min.x;

            match &mut self.kind {
                ModKind::Normal(normal_mod) => {
                    ui.checkbox(&mut normal_mod.enabled, "")
                        .on_hover_text("Enabled");
                }
                _ => {}
            }
            let cursor_checkbox_end = ui.cursor().min.x;
            let checkbox_space_to_do = 30.0 + cursor_checkbox_start - cursor_checkbox_end;
            ui.allocate_space(vec2(checkbox_space_to_do, 0.0));

            let cursor_icon_start = ui.cursor().min.x;
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
                        })
                        .rect
                        .width();
                    }
                }
                ModSource::Steam(steam_mod) => {
                    let steam_url = "https://steamcommunity.com/sharedfiles/filedetails/?id="
                        .to_owned()
                        + &steam_mod.workshop_id;
                    ui.hyperlink_to(format!("{STEAM}"), &steam_url)
                        .on_hover_text(format!("Steam ({steam_url})"))
                        .rect
                        .width();
                }
                _ => {}
            }
            let cursor_icon_end = ui.cursor().min.x;
            let icons_space_to_do = 40.0 + cursor_icon_start - cursor_icon_end;
            ui.allocate_space(vec2(icons_space_to_do, 0.0));

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
                    ui.label(
                        RichText::new(format!("{UNSAFE}")).color(Color32::from_rgb(255, 220, 40)),
                    )
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
        });
    }
}

#[derive(Clone, Debug)]
struct Popup<'a> {
    content: String,
    title: &'a str,
    id: usize,
}

impl<'a> Popup<'a> {
    /// returns if the popup is still open
    fn show(&self, ctx: &egui::Context) -> bool {
        let mut open = true;
        Window::new(self.title)
            .id(Id::new(self.id))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(&self.content);
            });
        open
    }
}

struct App<'a, 'b> {
    search: String,
    mod_config: &'a Path,
    mods: Vec<Mod>,
    popups: Vec<Popup<'b>>,
    global_id: usize,
}

#[derive(Clone, Debug)]
struct ModConfigItem {
    id: String,
    /// This is from the config, so the bool might just be nonsense if it's not a normal mod
    enabled: bool,
}

impl App<'_, '_> {
    fn create_error(&mut self, error: anyhow::Error) {
        println!("Error: {error:?}");
        self.popups.push(Popup {
            title: "Error",
            content: format!("{error:?}"),
            id: self.global_id,
        });
        self.global_id += 1;
    }

    /// call this to sort the loaded mods by a config, must have loaded some mods for this to do anything
    fn sort_mods(&mut self, mod_config: &Vec<ModConfigItem>) -> anyhow::Result<()> {
        let mut mod_map = HashMap::new();
        for nmod in self.mods.iter() {
            if mod_map.insert(nmod.id.clone(), nmod).is_some() {
                bail!(
                    "Duplicate mod id {} in loaded mods, mod list is broken",
                    &nmod.id
                );
            }
        }

        let mut new_mods = Vec::new();
        for config_item in mod_config.iter() {
            if let Some(got_mod) = mod_map.get(&config_item.id) {
                let mod_enabled = if let ModKind::Normal(normal_mod) = &got_mod.kind {
                    let mut new_mod = (*got_mod).clone();
                    let mut new_kind = *normal_mod;
                    new_kind.enabled = config_item.enabled;
                    new_mod.kind = ModKind::Normal(new_kind);
                    new_mod
                } else {
                    (*got_mod).clone()
                };
                new_mods.push(mod_enabled);
            }
        }

        self.mods = new_mods;
        Ok(())
    }

    fn parse_config_item(node: &XMLNode) -> anyhow::Result<ModConfigItem> {
        let element = node
            .as_element()
            .context("Couldn't convert xmlnode to element?")?;
        let name = element.attributes.get("name").context("Missing name")?;
        let enabled = element
            .attributes
            .get("enabled")
            .context("Missing enabled")?
            == "1";
        Ok(ModConfigItem {
            id: name.clone(),
            enabled,
        })
    }

    fn parse_config<R>(src: R) -> anyhow::Result<Vec<ModConfigItem>>
    where
        R: Read,
    {
        let tree = Element::parse(src)?;
        tree.children
            .iter()
            .map(|x| Self::parse_config_item(x))
            .try_fold(Vec::new(), |mut acc, x| {
                acc.push(x?);
                Ok(acc)
            })
    }

    fn load_mod(path: &Path, is_workshop: bool) -> anyhow::Result<Option<Mod>> {
        let mod_xml = path.join("mod.xml");
        if !mod_xml.is_file() {
            return Ok(None);
        }

        let file = File::open(mod_xml).context("Opening mod xml")?;
        let reader = BufReader::new(file);
        // TODO: port NXML to rust and use it here
        let tree = Element::parse(reader).context("Parsing mod xml")?;
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
                .context(format!("Opening mod_id.txt for {suffix}"))?
                .read_to_string(&mut id)
                .context(format!("Reading mod_id.txt for {suffix}"))?;
            ModSource::Steam(SteamMod {
                workshop_id: suffix.clone(),
            })
        } else if path.join(".git").is_dir() {
            let repo = git2::Repository::discover(path).context("Finding git repo")?;
            let remotes = repo.remotes().context("Getting git remotes")?;
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
            kind: if get(&tree, "is_translation".to_owned(), "0".to_owned()) == "1" {
                ModKind::Translation
            } else if get(&tree, "is_game_mode".to_owned(), "0".to_owned()) == "1" {
                ModKind::Gamemode
            } else {
                ModKind::Normal(NormalMod { enabled: false })
            },
            settings_fold_open: get(&tree, "settings_fold_open".to_string(), "0".to_owned()) == "1",
            name: get(&tree, "name".to_owned(), "unnamed".to_owned()),
            description: get(&tree, "description".to_owned(), "".to_owned()).replace("\\n", "\n"),
            unsafe_api: get(
                &tree,
                "request_no_api_restrictions".to_owned(),
                "0".to_owned(),
            ) == "1",
        };
        Ok(Some(nmod))
    }

    fn load_dir(&mut self, dir: &Path, is_workshop: bool) -> anyhow::Result<()> {
        fs::read_dir(dir)
            .context("Reading mods directory")?
            .try_for_each::<_, anyhow::Result<()>>(|item| {
                let item = item.context("Getting directory item")?;
                let path = item.path();
                if !path.is_dir() {
                    return Ok(());
                }
                let nmod = Self::load_mod(&path, is_workshop).context({
                    format!(
                        "Loading mod with path {}",
                        path.to_str()
                            .context("Producing a path string from a Path")?
                    )
                })?;
                {
                    if let Some(x) = nmod {
                        self.mods.push(x);
                    }
                    Ok(())
                }
            })?;
        Ok(())
    }

    pub fn new<'a, 'b>(mod_config: &'a Path) -> App<'a, 'b> {
        return App {
            mod_config,
            search: "".to_owned(),
            mods: Vec::new(),
            popups: Vec::new(),
            global_id: 0,
        };
    }

    fn save_mods(&self) -> anyhow::Result<()> {
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
        let mut file = File::create(self.mod_config).context("Opening mod config for saving")?;
        write!(file, "{}", buf).context("Writing to mod config")?;
        file.flush().context("Flushing file")?;
        Ok(())
    }
}

trait RetainEnumerateExt<T> {
    fn retain_enumerate<F>(&mut self, f: F)
    where
        F: FnMut(&T, usize) -> bool;
}

impl<T> RetainEnumerateExt<T> for Vec<T> {
    fn retain_enumerate<F>(&mut self, mut f: F)
    where
        F: FnMut(&T, usize) -> bool,
    {
        let mut i: usize = 0;
        self.retain(|e| {
            let result = f(e, i);
            i += 1;
            result
        });
    }
}

impl eframe::App for App<'_, '_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.popups.retain(|popup| popup.show(&ctx));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Mod Manager");
            if ui
                .button("Save")
                .on_hover_text("Save mod config for use in game (requires restarting Noita)")
                .clicked()
            {
                if let Err(error) = self.save_mods().context("While saving mod config") {
                    self.create_error(error);
                }
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
            let conditions: &Vec<_> = &conditions_err.iter().filter_map(|x| x.1.clone()).collect();
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
                    for (i, nmod) in self
                        .mods
                        .iter_mut()
                        .filter(|x| x.matches(&conditions))
                        .enumerate()
                    {
                        nmod.render(ui)
                    }
                });
        });
    }
}
