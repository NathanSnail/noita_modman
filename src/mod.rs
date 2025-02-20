use std::collections::HashSet;

use conditional::Condition;
use egui::{Color32, Rect, RichText};
pub mod conditional;
use crate::app::UiSizedExt;

const STEAM: char = '\u{E623}';
const TRANSLATION: char = '\u{1F4D5}';
const GAMEMODE: char = '\u{1F30F}';
const NORMAL: char = '\u{1F5A5}';
const UNSAFE: char = '\u{26A0}';

#[derive(Copy, Clone, Debug)]
pub enum GitHost {
    Github,
    Gitlab,
    Other,
}

#[derive(Clone, Debug)]
pub struct GitMod {
    pub remote: Option<String>,
    pub host: GitHost,
}

#[derive(Clone, Debug)]
pub struct SteamMod {
    pub workshop_id: String,
}

#[derive(Clone, Debug)]
pub struct ModWorkshopMod {
    pub link: String,
}

#[derive(Clone, Debug)]
pub enum ModSource {
    Git(GitMod),
    Steam(SteamMod),
    ModWorkshop(ModWorkshopMod),
    Manual,
}

#[derive(Copy, Clone, Debug)]
pub struct NormalMod {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum ModKind {
    Normal(NormalMod),
    Translation,
    Gamemode,
}

#[derive(Clone, Debug)]
pub struct Mod {
    pub source: ModSource,
    pub kind: ModKind,
    pub name: String,
    pub id: String,
    pub description: String,
    pub unsafe_api: bool,
    /// this is just needed for saving as we loaded it
    pub settings_fold_open: bool,
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct ModRenderResponse {
    pub full_rect: Rect,
    pub text_rect: Rect,
    pub text_hover: String,
}

impl Mod {
    pub fn matches(&self, conditions: &[Condition]) -> bool {
        conditions
            .iter()
            .map(|x| x.matches(&self))
            .reduce(|a, b| a && b)
            .unwrap_or(true)
    }

    // returns the rect of the text and it's hover text for dragging
    pub fn render(&mut self, ui: &mut egui::Ui) -> ModRenderResponse {
        let full = ui.horizontal(|ui| {
            ui.fixed_size_group(28.0, |ui| match &mut self.kind {
                ModKind::Normal(normal_mod) => {
                    ui.checkbox(&mut normal_mod.enabled, "")
                        .on_hover_text("Enabled");
                }
                _ => {}
            });

            ui.fixed_size_group(30.0, |ui| match &self.source {
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
            });

            ui.fixed_size_group(60.0, |ui| {
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
                            RichText::new(format!("{UNSAFE}"))
                                .color(Color32::from_rgb(255, 220, 40)),
                        )
                        .on_hover_text("Unsafe mod");
                    }
                });
            });

            let hover = "(".to_owned()
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
                + &self.description;
            let text_rect = ui.label(&self.name).rect;
            (text_rect, hover)
        });
        ModRenderResponse {
            full_rect: full.response.rect,
            text_rect: full.inner.0,
            text_hover: full.inner.1,
        }
    }
}
