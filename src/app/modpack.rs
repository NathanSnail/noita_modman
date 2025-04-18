use modsettings::{ModSetting, ModSettingPair, ModSettingValue, ModSettings};
use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    io::{Read, Write},
    iter::zip,
};

use anyhow::{anyhow, bail, Context, Error};
use egui::{Id, InnerResponse, Rect, RichText, Ui};
use fastlz;

use crate::{
    app::{ModListConfig, UiSizedExt},
    collapsing_ui::CollapsingUi,
    ext::{
        ByteReaderExt, ByteVec, ByteWriterExt,
        Endianness::{Big, Little},
    },
    icons::{UNSAFE, YELLOW},
    r#mod::ModKind,
};

use super::SCALE;
pub mod modsettings;

#[derive(Clone, Debug, PartialEq)]
pub struct TogglableSetting {
    pair: ModSettingPair,
    include: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModSettingsGroup(Vec<(String, ModSettingsNode)>);

#[derive(Clone, Debug, PartialEq)]
enum ModSettingsNode {
    Group(ModSettingsGroup),
    Setting(TogglableSetting),
}

impl ModSettingsGroup {
    pub fn to_set(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for child in self.0.iter() {
            let child_set = match &child.1 {
                ModSettingsNode::Group(mod_settings_group) => mod_settings_group
                    .to_set()
                    .iter()
                    .map(|e| ".".to_string() + e)
                    .collect(),
                ModSettingsNode::Setting(_) => HashSet::new(),
            };
            set = set.union(&child_set).map(|e| child.0.clone() + e).collect();
        }
        set
    }

    pub fn all_included(&self) -> bool {
        self.0.iter().fold(true, |acc, e| {
            acc && match &e.1 {
                ModSettingsNode::Group(mod_settings_group) => mod_settings_group.all_included(),
                ModSettingsNode::Setting(togglable_setting) => togglable_setting.include,
            }
        })
    }

    pub fn render(&mut self, ui: &mut Ui) {
        for (key, setting) in self.0.iter_mut() {
            match setting {
                ModSettingsNode::Group(mod_settings_group) => {
                    ui.push_id(Id::new(key as &str), |ui| {
                        let captured_key = key.clone();
                        let captured_checked = mod_settings_group.all_included();
                        let check_include = CollapsingUi::new(
                            Id::new("Top"),
                            Box::new(move |ui| {
                                ui.scope(|ui| {
                                    let mut checked = captured_checked;
                                    let rect = ui
                                        .horizontal(|ui| {
                                            ui.checkbox(&mut checked, "").on_hover_text(
                                                match captured_checked {
                                                    true => "Exclude all children of this node",
                                                    false => "Include all children of this node",
                                                },
                                            );
                                            ui.label(&captured_key).rect
                                        })
                                        .inner;
                                    (
                                        if checked != captured_checked {
                                            Some(checked)
                                        } else {
                                            None
                                        },
                                        rect,
                                    )
                                })
                            }),
                        )
                        .show(ui, |ui| mod_settings_group.render(ui))
                        .inner;

                        match check_include {
                            Some(check) => mod_settings_group.include_all(check),
                            None => (),
                        }
                    });
                }
                ModSettingsNode::Setting(togglable_setting) => {
                    let mut include = togglable_setting.include;
                    ui.checkbox(&mut include, key as &str).on_hover_ui(|ui| {
                        togglable_setting.pair.render(ui);
                    });
                    togglable_setting.include = include;
                }
            }
        }
    }

    pub fn sort(&mut self) {
        for child in self.0.iter_mut() {
            match &mut child.1 {
                ModSettingsNode::Group(mod_settings_group) => mod_settings_group.sort(),
                ModSettingsNode::Setting(_) => (),
            }
        }
        self.0.sort_by_key(|e| e.0.clone());
    }

    pub fn traverse<'a, 'b, I: Iterator<Item = &'a str>>(
        &'b mut self,
        mut path: I,
    ) -> &'b mut Self {
        let section = path.next();
        match section {
            Some(section) => {
                let parent = self
                    .0
                    .iter_mut()
                    .enumerate()
                    .filter(|e| &e.1 .0 == section && matches!(&e.1 .1, ModSettingsNode::Group(_)))
                    .filter_map(|e| match &mut e.1 .1 {
                        ModSettingsNode::Group(_) => Some(e.0),
                        _ => None,
                    })
                    .next();
                match parent {
                    Some(parent) => match &mut self.0[parent].1 {
                        ModSettingsNode::Group(mod_settings_group) => {
                            mod_settings_group.traverse(path)
                        }
                        ModSettingsNode::Setting(_) => unreachable!(),
                    },
                    None => {
                        self.0.push((
                            section.to_string(),
                            ModSettingsNode::Group(ModSettingsGroup(Vec::new())),
                        ));
                        let len = self.0.len();
                        match &mut self.0[len - 1].1 {
                            ModSettingsNode::Group(mod_settings_group) => {
                                mod_settings_group.traverse(path)
                            }
                            ModSettingsNode::Setting(_) => unreachable!(),
                        }
                    }
                }
            }
            None => self,
        }
    }

    pub fn include_all(&mut self, include: bool) {
        for (_, setting) in self.0.iter_mut() {
            match setting {
                ModSettingsNode::Group(mod_settings_group) => {
                    mod_settings_group.include_all(include)
                }
                ModSettingsNode::Setting(togglable_setting) => togglable_setting.include = include,
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModPack {
    file_name: String,
    name: String,
    mods: Vec<String>,
    settings: ModSettings,
}

fn decompress_file<R: Read>(mut reader: R, file_size: usize) -> anyhow::Result<Vec<u8>> {
    let compressed_size = reader.read_le::<u32>().context("Reading compressed size")?;
    if compressed_size as usize + 8 != file_size {
        bail!(
            "File should be {} when compressed according to content, but is actually {file_size}",
            compressed_size + 8
        );
    }

    let decompressed_size = reader
        .read_le::<u32>()
        .context("Reading decompressed size")?;
    let mut compressed = vec![0; compressed_size as usize];
    reader
        .read_exact(&mut compressed)
        .context("Reading the compressed data to a vec")?;
    if compressed_size != decompressed_size {
        let mut output = vec![0; decompressed_size as usize];
        fastlz::decompress(&compressed, &mut output)
            .map_err(|_| anyhow!("FastLZ failed to decompress"))?;
        Ok(output)
    } else {
        Ok(compressed)
    }
}

fn compress_file<W: Write>(mut writer: W, buf: &[u8]) -> anyhow::Result<()> {
    let mut our_buf = buf;
    let mut new_buf = [0; 16];
    if buf.len() < 16 {
        new_buf[..buf.len()].copy_from_slice(buf);
        our_buf = &new_buf;
    }
    let mut output = vec![0; max(buf.len() * 2, 128)]; // apparently 5% and 66 bytes is safe, but i have 0 trust of that
    let output_slice =
        fastlz::compress(our_buf, &mut output).map_err(|_| anyhow!("FastLZ failed to compress"))?;
    if output_slice.len() >= buf.len() {
        writer
            .write_le::<u32>(buf.len() as u32)
            .context("Writing output length")?;
        writer
            .write_le::<u32>(buf.len() as u32)
            .context("Writing input length")?;
        writer.write_all(buf).context("Writing compressed buffer")?;
    } else {
        writer
            .write_le::<u32>(output_slice.len() as u32)
            .context("Writing output length")?;
        writer
            .write_le::<u32>(buf.len() as u32)
            .context("Writing input length")?;
        writer
            .write_all(output_slice)
            .context("Writing compressed buffer")?;
    }
    Ok(())
}

impl ModPack {
    fn load_v0<R: Read>(mut reader: R, file_name: String) -> anyhow::Result<ModPack> {
        let name = reader
            .read_str::<usize>(Little)
            .context("Reading modpack name")?;
        let err_name = name.clone();
        (|| {
            let num_mods = reader
                .read_le::<usize>()
                .context(format!("Reading modpack number of mods"))?;

            let mut mods = Vec::with_capacity(num_mods);

            for _ in 0..num_mods {
                let mod_name = reader
                    .read_str::<usize>(Little)
                    .context("Reading mod name")?;
                mods.push(mod_name);
            }

            let num_settings = reader
                .read_le::<usize>()
                .context("Reading modpack number of settings")?;

            let mut settings = HashMap::new();
            for i in 0..num_settings {
                let setting = ModSetting::load(&mut reader)
                    .context(format!("Loading modpack setting {i}"))?;
                settings.insert(setting.key, setting.values);
            }

            Ok::<ModPack, Error>(ModPack {
                file_name,
                name,
                mods,
                settings: ModSettings {
                    values: settings,
                    ..Default::default()
                },
            })
        })()
        .context(format!("Loading pack {err_name}"))
    }

    pub fn apply(&self, mod_list_config: &mut ModListConfig) {
        let mut enabled = HashMap::new();
        for (i, nmod) in self.mods.iter().enumerate() {
            enabled.insert(nmod, i);
        }

        // TODO: make this fast with swaps
        let mut enabled_mods = Vec::new();
        let mut enabled_idxs = Vec::new();
        for (i, nmod) in mod_list_config.mods.iter_mut().enumerate() {
            if let ModKind::Normal(normal_mod) = &mut nmod.kind {
                if let Some(v) = enabled.get(&nmod.id) {
                    normal_mod.enabled = true;
                    enabled_mods.push((nmod.clone(), *v));
                    enabled_idxs.push(i);
                } else {
                    normal_mod.enabled = false;
                }
            }
        }

        enabled_mods.sort_by_key(|e| e.1);
        for (nmod, idx) in zip(enabled_mods, enabled_idxs) {
            mod_list_config.mods[idx] = nmod.0;
        }

        for (key, values) in self.settings.values.iter() {
            mod_list_config
                .mod_settings
                .values
                .insert(key.clone(), values.clone());
        }
    }

    pub fn load<R: Read>(mut reader: R, file_name: String) -> anyhow::Result<ModPack> {
        let version = reader
            .read_le::<usize>()
            .context("Reading modpack schema version")?;
        match version {
            0 => Self::load_v0(reader, file_name),
            1.. => bail!("Attempted to load future modpack schema (v{version})"),
        }
    }

    pub fn save<W: Write>(&self, mut writer: W, include: &ModSettingsGroup) -> anyhow::Result<()> {
        (|| {
            writer
                .write_le::<usize>(0)
                .context("Writing modpack schema version")?;
            writer
                .write_str::<usize>(&self.name, Little)
                .context("Writing modpack data")?;
            writer
                .write_le::<usize>(self.mods.len())
                .context("Writing modpack number of mods")?;

            for nmod in self.mods.iter() {
                writer
                    .write_le::<usize>(nmod.len())
                    .context("Writing mod name length")?;
                writer
                    .write_all(nmod.as_bytes())
                    .context("Writing mod name")?;
            }

            writer
                .write_le::<usize>(self.settings.values.len())
                .context("Writing modpack number of settings")?;

            let set = include.to_set();
            for (key, values) in self
                .settings
                .values
                .iter()
                .filter(|(key, _)| set.contains(*key))
            {
                ModSetting {
                    key: key.clone(),
                    values: values.clone(),
                }
                .save(&mut writer)
                .context(format!("Saving setting {key}"))?;
            }

            Ok::<_, Error>(())
        })()
        .context(format!("Saving pack {}", self.name))
    }

    /// Returns an optional error message which should be displayed, can't borrow `&mut App` because we need to iterate over modpacks when calling this
    /// If you are doing a sizing pass to get the row rect, shade_bg must be false
    // TODO: make shade_bg an Option<(bool, Rect)> type deal
    pub fn render(
        &self,
        ui: &mut Ui,
        mod_list: &mut ModListConfig,
        search_term: &mut String,
        installed: &HashSet<String>,
        shade_bg: bool,
        row_rect: Option<Rect>,
    ) -> InnerResponse<Option<String>> {
        ui.horizontal(|ui| {
            if shade_bg {
                let painter = ui.painter();

                let mut cursor = ui.cursor();
                cursor.max.y = cursor.min.y + row_rect.unwrap().height();
                painter.rect_filled(cursor, 0.0, ui.visuals().faint_bg_color);
            }

            let mut error: Option<String> = None;
            for nmod in self.mods.iter() {
                if !installed.contains(nmod) {
                    error = Some(
                        error
                            .clone() // TODO: this is not needed, find a way to fix
                            .map_or_else(|| nmod.clone(), |e| e + "\n" + nmod),
                    );
                }
            }
            error = error.map(|e| "Missing mods:\n".to_owned() + &e);

            let result = if ui.button("Apply").clicked() {
                *search_term = self.name.clone();
                self.apply(mod_list);
                if let Some(err) = &error {
                    Some(err.clone())
                } else {
                    None
                }
            } else {
                None
            };

            ui.fixed_size_group(40.0 * SCALE, |ui| {
                if let Some(err) = &error {
                    ui.label(RichText::new(format!("{UNSAFE}")).color(YELLOW))
                        .on_hover_text(err);
                }
            });

            ui.label(&self.name).on_hover_ui(|ui| {
                ui.label(format!("({})\n", &self.file_name));
                for nmod in self.mods.iter() {
                    ui.label(nmod);
                }
            });

            result
        })
    }

    pub fn new(
        name: String,
        file_name: String,
        mods: &[String],
        settings: &ModSettings,
    ) -> ModPack {
        ModPack {
            file_name,
            name,
            mods: mods.to_vec(),
            settings: settings.clone(),
        }
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ModSetting {
    pub fn load<R: Read>(mut reader: R) -> anyhow::Result<Self> {
        let key = reader.read_str::<u32>(Big).context("Reading key")?;
        let setting_current_type = reader
            .read_be::<u32>()
            .context(format!("Reading setting {key} current type"))?;
        let setting_next_type = reader
            .read_be::<u32>()
            .context(format!("Reading setting {key} next type"))?;
        let setting_current = ModSettingValue::load(&mut reader, setting_current_type)
            .context(format!("Reading setting {key} current value"))?;
        let setting_next = ModSettingValue::load(&mut reader, setting_next_type)
            .context(format!("Reading setting {key} next value"))?;
        Ok(ModSetting {
            key,
            values: ModSettingPair {
                current: setting_current,
                next: setting_next,
            },
        })
    }

    pub fn save<W: Write>(&self, mut writer: W) -> anyhow::Result<()> {
        (|| {
            writer
                .write_be::<u32>(self.key.len() as u32)
                .context("Writing key length")?;
            let key_buf = self.key.as_bytes();
            writer.write_all(key_buf).context("Writing key")?;
            writer
                .write_be::<u32>(self.values.current.type_int())
                .context("Writing current type")?;
            writer
                .write_be::<u32>(self.values.next.type_int())
                .context("Writing next type")?;
            self.values
                .current
                .save(&mut writer)
                .context("Writing current value")?;
            self.values
                .next
                .save(&mut writer)
                .context("Writing next value")?;
            Ok::<_, Error>(())
        })()
        .context(format!("Writing setting with key {}", self.key))
    }
}

impl ModSettings {
    // basically a port of dexters https://github.com/dextercd/NoitaSettings/blob/main/settings_main.cpp
    pub fn load<R: Read>(reader: R, file_size: usize) -> anyhow::Result<ModSettings> {
        let mut settings = HashMap::new();
        let mut decompressed =
            ByteVec(decompress_file(reader, file_size).context("Decompressing file")?);
        let expected_num_entries = decompressed
            .read_be::<u64>()
            .context("Reading expected entries")?;
        let mut num_entries = 0;
        while decompressed.0.len() != 0 {
            let setting = ModSetting::load(&mut decompressed)
                .context(format!("Loading setting number {num_entries}"))?;
            num_entries += 1;
            settings.insert(setting.key, setting.values);
        }

        if num_entries != expected_num_entries {
            bail!("Expected {expected_num_entries} but there were {num_entries}");
        }
        let settings = ModSettings {
            grouped: Self::compute_grouped(&settings),
            values: settings,
        };
        Ok(settings)
    }

    pub fn save<W: Write>(&self, writer: W) -> anyhow::Result<()> {
        let mut buf = ByteVec(Vec::new());
        buf.write_be::<u64>(self.values.len() as u64)
            .context("Writing number of settings")?;
        for (key, values) in self.values.iter() {
            let setting = ModSetting {
                key: key.clone(),
                values: values.clone(),
            };
            setting.save(&mut buf)?; // TODO: remove clones
        }
        compress_file(writer, &buf.0).context("Compressing to file")
    }

    pub fn render(&mut self, ui: &mut Ui) {
        self.grouped.render(ui);
    }

    fn compute_grouped(map: &HashMap<String, ModSettingPair>) -> ModSettingsGroup {
        let mut tree: ModSettingsGroup = ModSettingsGroup(Default::default());
        for (key, pair) in map.iter() {
            let parts = key.split('.').collect::<Vec<_>>();
            let suffix = *parts
                .last()
                .expect("A split string should have at least one part");
            let prefix = parts[0..parts.len() - 1].iter().map(|e| *e);
            let node = tree.traverse(prefix);
            node.0.push((
                suffix.to_string(),
                ModSettingsNode::Setting(TogglableSetting {
                    pair: pair.clone(),
                    include: false,
                }),
            ))
        }
        tree.sort();
        tree
    }

    pub fn grouped(&self) -> &ModSettingsGroup {
        &self.grouped
    }
}

#[cfg(test)]
mod test {
    use super::{compress_file, decompress_file};
    use crate::ext::ByteVec;

    #[test]
    fn compress() {
        let s = "\u{fff4}\u{2000}\u{fff4}⁀ࠀ\0\0\0\0".as_bytes();
        let mut buffer = ByteVec(Vec::new());
        compress_file(&mut buffer, s).expect("Saving must work");
        let len = buffer.0.len();
        let decompressed = decompress_file(&mut buffer, len).expect("Loading must work");
        assert_eq!(s, decompressed);
    }
}
