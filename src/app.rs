use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    marker::PhantomData,
    path::Path,
};

use anyhow::{anyhow, bail, Context};
use egui::{
    emath, vec2, Button, Color32, DragAndDrop, FontFamily, FontId, Grid, Id, InnerResponse,
    LayerId, Order, Rangef, Rect, Sense, TextStyle, Ui, UiBuilder, Window,
};
use modpack::{modsettings::ModSettings, ModPack};

use xmltree::{Element, XMLNode};

use crate::r#mod::{
    conditional::Condition, GitHost, GitMod, Mod, ModKind, ModSource, NormalMod, SteamMod,
};

mod modpack;

pub const SCALE: f32 = 1.6;

#[derive(Copy, Clone, Debug)]
struct DNDPayload(usize);
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

struct ModListConfig {
    search: String,
    mods: Vec<Mod>,
    mod_settings: ModSettings,
}

struct ModPackConfig {
    name: String,
    modpacks: Vec<ModPack>,
    row_rect: Option<Rect>,
    installed_mods: HashSet<String>,
}

pub struct App<'a, 'b, 'c> {
    list_config: ModListConfig,
    pack_config: ModPackConfig,

    mod_config: &'a Path,
    mods_dir: Option<&'a Path>,
    workshop_dir: Option<&'a Path>,
    mod_settings_file: &'a Path,
    popups: Vec<Popup<'b>>,
    global_id: usize,
    row_rect: Option<Rect>,
    init_errored: bool,

    #[allow(dead_code)]
    profiler: ProfilerInfo<'c>,
}

#[cfg(feature = "profiler")]
pub struct ProfilerInfo<'a> {
    pub frame_counter: u64,
    pub profiler: pprof::ProfilerGuard<'a>,
}

#[cfg(not(feature = "profiler"))]
pub struct ProfilerInfo<'a> {
    pub profiler: PhantomData<&'a ()>,
}

#[derive(Clone, Debug)]
pub struct ModConfigItem {
    pub id: String,
    /// This is from the config, so the bool might just be nonsense if it's not a normal mod
    pub enabled: bool,
}

impl<'d, 'e, 'f> App<'d, 'e, 'f> {
    fn render_modpack_panel(&mut self, ui: &mut Ui) -> anyhow::Result<()> {
        if self.pack_config.row_rect == None {
            if let Some(pack) = self.pack_config.modpacks.get_mut(0) {
                self.pack_config.row_rect = Some(
                    pack.render(
                        ui,
                        &mut self.list_config,
                        &mut "".to_owned(),
                        &HashSet::new(),
                        false,
                        None,
                    )
                    .response
                    .rect,
                );
                ui.ctx().request_repaint();
            }
        }

        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut self.pack_config.name);
        });
        if ui.button("Export as modpack").clicked() {
            let pack = ModPack::new(
                self.pack_config.name.clone(),
                self.pack_config.name.clone(),
                &self
                    .list_config
                    .mods
                    .iter()
                    .filter(|e| {
                        if let ModKind::Normal(nmod) = e.kind {
                            nmod.enabled
                        } else {
                            false
                        }
                    })
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>(),
                &self.list_config.mod_settings,
            );
            let path = Path::new("./modpacks/").join(&self.pack_config.name);
            pack.save(BufWriter::new(File::create(path).context(format!(
                "Creating modpack {}",
                &self.pack_config.name
            ))?))
            .context(format!("Saving modpack {}", &self.pack_config.name))?;
            if let Some(found) = self
                .pack_config
                .modpacks
                .iter_mut()
                .find(|e| e.file_name() == pack.file_name())
            {
                *found = pack;
            } else {
                self.pack_config.modpacks.push(pack);
            }
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut error = None;
                let searching_name = self.pack_config.name.clone();
                Grid::new("Modpack Grid").striped(false).show(ui, |ui| {
                    for (i, modpack) in self
                        .pack_config
                        .modpacks
                        .iter()
                        .filter(|e| e.name().contains(&searching_name))
                        .enumerate()
                    {
                        // if we just saved the first pack then row rect can be in a bad state here, just draw a frame later
                        if self.pack_config.row_rect == None {
                            return;
                        }
                        if let Some(err) = modpack
                            .render(
                                ui,
                                &mut self.list_config,
                                &mut self.pack_config.name,
                                &self.pack_config.installed_mods,
                                i % 2 == 0,
                                self.pack_config.row_rect,
                            )
                            .inner
                        {
                            error = Some(err);
                        }
                        ui.end_row();
                    }
                });
                if let Some(err) = error {
                    self.create_error(anyhow!(err));
                }
                Ok(())
            })
            .inner
    }

    fn render_mod_settings_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                self.list_config.mod_settings.render(ui);
            });
    }

    fn render_mods_panel(&mut self, ui: &mut Ui) {
        if self.row_rect == None {
            if let Some(nmod) = self.list_config.mods.get_mut(0) {
                self.row_rect = Some(nmod.render(ui, self.init_errored).full_rect);
                ui.ctx().request_repaint();
            }
        }

        let cur_search = self.list_config.search.clone();
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
            ui.text_edit_singleline(&mut self.list_config.search)
                .on_hover_text(Condition::special_terms());
            if !broken_terms.is_empty() {
                ui.label("Broken search terms: ");
                broken_terms.iter().for_each(|x| {
                    ui.label(x.to_string());
                });
            }
        });
        if ui
            .add_enabled(!self.init_errored, Button::new("Save"))
            .on_hover_text("Save mod config for use in game (requires restarting Noita)")
            .on_disabled_hover_text("Cannot save when there was an error starting the mod manager, fix the errors then save.")
            .clicked()
        {
             let res = self.save_mods().context("While saving mod config") ;
             self.result_popup(res);
        }

        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| self.render_dnd_modlist(ui, conditions));
    }

    fn result_popup<T>(&mut self, error: anyhow::Result<T>) {
        if let Err(e) = error {
            self.create_error(e);
        }
    }

    fn create_error(&mut self, error: anyhow::Error) {
        println!("Error: {error:?}");
        self.popups.push(Popup {
            title: "Error",
            content: format!("{error:?}"),
            id: self.global_id,
        });
        self.global_id += 1;
    }

    fn load_modpacks(&mut self, dir: &Path) -> anyhow::Result<()> {
        let mut packs = Vec::new();
        for file in fs::read_dir(dir).context(format!("Reading modpack dir {}", dir.display()))? {
            let file = file.context(format!("Accessing file for modpack dir {}", dir.display()))?;
            let file_name = file
                .file_name()
                .to_str()
                .context(format!("Getting file name {}", file.path().display()))?
                .to_string();
            if file_name.starts_with('.') {
                continue;
            }
            let reader = BufReader::new(
                File::open(file.path())
                    .context(format!("Opening modpack file {}", file.path().display()))?,
            );
            let pack = ModPack::load(reader, file_name).context(format!(
                "Loading modpack from file {}",
                file.path().display()
            ))?;
            packs.push(pack);
        }
        self.pack_config.modpacks = packs;
        Ok(())
    }

    fn render_dnd_modlist(&mut self, ui: &mut Ui, conditions: &[Condition]) {
        let payload = egui::DragAndDrop::take_payload::<DNDPayload>(ui.ctx()); // taking the payload clears it
        let inner_response = self.render_modlist(ui, conditions, payload.is_some());

        if ui.ctx().input(|i| i.pointer.any_down()) {
            return;
        }

        if let Some(dnd_payload) = payload {
            let to_idx = inner_response.inner;
            let from_idx = dnd_payload.0;
            if let Some(to_idx) = to_idx {
                if from_idx == to_idx {
                    return;
                }
                let filtered_mods = self
                    .list_config
                    .mods
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.matches(conditions))
                    .collect::<Vec<_>>();
                let mut target_mod_idx = if to_idx == 0 {
                    // if we drag it to the start always put it at the start
                    0
                } else {
                    filtered_mods
                        .iter()
                        .skip(to_idx)
                        .take(1)
                        .collect::<Vec<_>>()
                        .get(0)
                        .map(|e| e.0)
                        .unwrap_or(self.list_config.mods.len()) // if we drag it to the bottom when filtered we probably want it at the end of the modlist
                };

                let from_mod_idx = filtered_mods
                    .get(from_idx)
                    .expect("Dragged mod should exist")
                    .0;

                let source = self.list_config.mods.remove(from_mod_idx);
                if target_mod_idx >= from_mod_idx {
                    target_mod_idx -= 1;
                }
                if target_mod_idx >= self.list_config.mods.len() {
                    self.list_config.mods.push(source);
                } else {
                    self.list_config.mods.insert(target_mod_idx, source);
                }
            }
        }
    }

    fn render_modlist(
        &mut self,
        ui: &mut Ui,
        conditions: &[Condition],
        do_dnd: bool,
    ) -> InnerResponse<Option<usize>> {
        ui.scope(|ui| {
            self.list_config
                .mods
                .iter_mut()
                .filter(|x| x.matches(conditions))
                .enumerate()
                .map(|(i, nmod)| {
                    let id = Id::new(("Modlist DND", i));
                    let payload = DNDPayload(i);

                    if i % 2 == 0 {
                        let painter = ui.painter();

                        let mut cursor = ui.cursor();
                        cursor.max.y = cursor.min.y + self.row_rect.unwrap().height();
                        painter.rect_filled(cursor, 0.0, ui.visuals().faint_bg_color);
                    }

                    // largely pilfered from Ui::dnd_drag_source
                    if ui.ctx().is_being_dragged(id) && !self.init_errored {
                        DragAndDrop::set_payload(ui.ctx(), payload);

                        let layer_id = LayerId::new(Order::Tooltip, id);
                        let response = ui
                            .scope_builder(UiBuilder::new().layer_id(layer_id), |ui| {
                                nmod.render(ui, self.init_errored)
                            })
                            .response;

                        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                            let delta = pointer_pos - response.rect.center();
                            ui.ctx().transform_layer_shapes(
                                layer_id,
                                emath::TSTransform::from_translation(delta),
                            );
                        }
                        None
                    } else {
                        let scoped = ui.scope(|ui| nmod.render(ui, self.init_errored));
                        let inner = scoped.inner;
                        ui.interact(inner.text_rect, id, Sense::drag())
                            .on_hover_cursor(if self.init_errored {
                                egui::CursorIcon::NotAllowed
                            } else {
                                egui::CursorIcon::Grab
                            })
                            .on_hover_text(inner.text_hover);
                        if do_dnd && scoped.response.contains_pointer() {
                            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                                let rect = scoped.response.rect;
                                let stroke = egui::Stroke::new(1.0, Color32::WHITE);
                                let x_range = Rangef {
                                    min: rect.x_range().min,
                                    max: 10000.0, // probably a better way to do this but idk how
                                };
                                if pointer.y > rect.center().y {
                                    ui.painter().hline(x_range, rect.bottom(), stroke);
                                    Some(i + 1)
                                } else {
                                    ui.painter().hline(x_range, rect.top(), stroke);
                                    Some(i)
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }
                })
                .fold(None, |acc, e| if acc.is_some() { acc } else { e })
        })
    }

    /// call this to sort the loaded mods by a config, must have loaded some mods for this to do anything
    fn sort_mods(mods: &[Mod], mod_config: &Vec<ModConfigItem>) -> anyhow::Result<Vec<Mod>> {
        let mut mod_map = HashMap::new();
        for nmod in mods.iter() {
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

        Ok(new_mods)
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

    fn parse_config<R: Read>(src: R) -> anyhow::Result<Vec<ModConfigItem>> {
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

        let mut tags = None;
        if let Ok(workshop) = File::open(path.join("workshop.xml")) {
            let reader = BufReader::new(workshop);
            let xml = Element::parse(reader).context("Parsing workshop.xml")?;
            let tags_str = get(&xml, "tags".to_owned(), "".to_owned());
            if tags_str != "" {
                // if it's default the mod doesn't support tags
                tags = Some(tags_str.split(',').map(|e| e.trim().to_owned()).collect());
            }
        }

        let nmod = Mod {
            source,
            tags,
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

    fn load_dir(dir: &Path, is_workshop: bool) -> anyhow::Result<Vec<Mod>> {
        let mut mods = Vec::new();
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
                        mods.push(x);
                    }
                    Ok(())
                }
            })?;
        Ok(mods)
    }

    fn init(&mut self) -> anyhow::Result<()> {
        let mut mods = Vec::new();
        if let Some(dir) = self.mods_dir {
            mods.extend(
                Self::load_dir(dir, false)
                    .context(format!("Loading mods dir {}", dir.display()))?,
            );
        }
        if let Some(dir) = self.workshop_dir {
            mods.extend(
                Self::load_dir(dir, true)
                    .context(format!("Loading workshop mods dir {}", dir.display()))?,
            );
        }

        let config = Self::parse_config(BufReader::new(
            File::open(self.mod_config)
                .context(format!("Opening mod config {}", self.mod_config.display()))?,
        ))
        .context(format!("Parsing mod config {}", self.mod_config.display()))?;
        self.list_config.mods = Self::sort_mods(&mods, &config).context("Sorting mods")?;

        let file = BufReader::new(File::open(self.mod_settings_file).context(format!(
            "Opening mod settings {}",
            self.mod_settings_file.display()
        ))?);
        self.list_config.mod_settings = ModSettings::load(
            file,
            fs::metadata(self.mod_settings_file)
                .context(format!(
                    "Getting metadata for mod settings {}",
                    self.mod_settings_file.display()
                ))?
                .len() as usize,
        )
        .context(format!(
            "Loading mod settings {}",
            self.mod_settings_file.display()
        ))?;
        self.load_modpacks(Path::new("./modpacks/"))
            .context("Loading modpacks")?;
        // mod_settings.save(BufWriter::new(File::create("./saved_settings")?))?;
        let installed = self
            .list_config
            .mods
            .iter()
            .map(|e| e.id.clone())
            .collect::<HashSet<_>>();
        self.pack_config.installed_mods = installed;
        Ok(())
    }

    pub fn new(
        mod_config: &'d Path,
        workshop_dir: Option<&'d Path>,
        mods_dir: Option<&'d Path>,
        mod_settings: &'d Path,
        profiler: ProfilerInfo<'f>,
    ) -> anyhow::Result<App<'d, 'e, 'f>> {
        Ok(Self {
            mod_config,
            list_config: ModListConfig {
                search: "".to_owned(),
                mods: Vec::new(),
                mod_settings: Default::default(),
            },
            mods_dir,
            workshop_dir,
            mod_settings_file: mod_settings,
            popups: Vec::new(),
            global_id: 0,
            row_rect: None,
            pack_config: ModPackConfig {
                name: "".to_owned(),
                modpacks: Vec::new(),
                row_rect: None,
                installed_mods: HashSet::new(),
            },
            init_errored: false,
            profiler,
        })
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        if let Err(e) = self.init() {
            self.create_error(e);
            self.init_errored = true;
        }

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
                        TextStyle::Small,
                        FontId::new(9.0 * SCALE, FontFamily::Proportional),
                    );
                    style.text_styles.insert(
                        TextStyle::Body,
                        FontId::new(12.5 * SCALE, FontFamily::Proportional),
                    );
                    style.text_styles.insert(
                        TextStyle::Button,
                        FontId::new(12.5 * SCALE, FontFamily::Proportional),
                    );
                    style.text_styles.insert(
                        TextStyle::Heading,
                        FontId::new(18.0 * SCALE, FontFamily::Proportional),
                    );
                    style.text_styles.insert(
                        TextStyle::Monospace,
                        FontId::new(12.0 * SCALE, FontFamily::Monospace),
                    );
                    style.spacing.interact_size *= SCALE;
                    style.spacing.icon_width *= SCALE;
                    style.spacing.icon_spacing *= SCALE;
                });
                Ok(Box::new(self))
            }),
        );

        result.map_err(|x| anyhow!(format!("{x:?}")))
    }

    fn save_mods(&self) -> anyhow::Result<()> {
        let buf = "<Mods>\n".to_string()
                    + &self
                        .list_config.mods
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

pub trait UiSizedExt {
    fn fixed_size_group<F: FnOnce(&mut Self)>(&mut self, size: f32, f: F);
}

impl UiSizedExt for egui::Ui {
    fn fixed_size_group<F: FnOnce(&mut Self)>(&mut self, size: f32, f: F) {
        let cursor_start = self.cursor().min.x;
        f(self);
        let cursor_end = self.cursor().min.x;
        self.allocate_space(vec2(size + cursor_start - cursor_end, 0.0));
    }
}

impl eframe::App for App<'_, '_, '_> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "profiler")]
        {
            self.profiler.frame_counter += 1;
            if self.profiler.frame_counter % 1000 == 0 {
                if let Ok(report) = self.profiler.profiler.report().build() {
                    let file = File::create("flamegraph.svg").unwrap();
                    report.flamegraph(file).unwrap();
                };
            }
        }

        self.popups.retain(|popup| popup.show(&ctx));

        egui::SidePanel::right(Id::new("Right Panel")).show(ctx, |ui| {
            self.render_mod_settings_panel(ui);
        });
        egui::TopBottomPanel::bottom(Id::new("Modpack Panel"))
            .resizable(true)
            .show(ctx, |ui| {
                let res = self.render_modpack_panel(ui);
                self.result_popup(res)
            });

        egui::CentralPanel::default().show(ctx, |ui| self.render_mods_panel(ui));
    }
}
