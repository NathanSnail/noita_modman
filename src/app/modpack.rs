use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    io::{Read, Write},
};

use anyhow::{anyhow, bail, Context, Error};
use egui::Ui;
use fastlz;

use crate::{
    app::{ModListConfig, UiSizedExt},
    ext::{ByteReaderExt, ByteWriterExt},
    icons::UNSAFE,
    r#mod::ModKind,
};

#[derive(Clone, Debug)]
enum ModSettingValue {
    /// id 0
    None,
    /// id 1
    Bool(bool),
    /// id 2
    Number(f64),
    /// id 3
    String(String),
}

#[derive(Clone, Debug)]
struct ModSettingPair {
    current: ModSettingValue,
    next: ModSettingValue,
}

#[derive(Clone, Debug)]
struct ModSetting {
    key: String,
    values: ModSettingPair,
}

#[derive(Clone, Debug)]
pub struct ModSettings(HashMap<String, ModSettingPair>);

#[derive(Clone, Debug)]
pub struct ModPack {
    name: String,
    mods: Vec<String>,
    settings: ModSettings,
}

#[derive(Clone, Debug)]
struct ByteVec(Vec<u8>);

impl Read for ByteVec {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = min(buf.len(), self.0.len());
        buf[..len].copy_from_slice(&self.0[..len]);
        self.0.drain(0..len);
        Ok(len)
    }
}

impl Write for ByteVec {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn decompress_file<R>(mut reader: R, file_size: usize) -> anyhow::Result<Vec<u8>>
where
    R: Read,
{
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
    let mut compressed = Vec::new();
    let result = reader.read_to_end(&mut compressed);
    if compressed_size == decompressed_size {
        match result {
            Ok(read) => {
                if read != decompressed_size as usize {
                    bail!("Expected to read {decompressed_size} when reading uncompressed file but read {read}");
                }
                return Ok(compressed);
            }
            Err(err) => return Err(err).context("Reading to end of file"),
        }
    }
    let mut output = vec![0; decompressed_size as usize];
    fastlz::decompress(&compressed, &mut output)
        .map_err(|_| anyhow!("FastLZ failed to decompress"))?;
    Ok(output)
}

fn compress_file<W>(mut writer: W, buf: &[u8]) -> anyhow::Result<()>
where
    W: Write,
{
    let mut output = vec![0; buf.len() * 2]; // according to dexter code fastlz isn't worse than this
    let output_slice =
        fastlz::compress(&(buf), &mut output).map_err(|_| anyhow!("FastLZ failed to compress"))?;
    writer
        .write_le::<u32>(output_slice.len() as u32)
        .context("Writing output length")?;
    writer
        .write_le::<u32>(buf.len() as u32)
        .context("Writing input length")?;
    writer
        .write_all(output_slice)
        .context("Writing compressed buffer")?;
    Ok(())
}

impl ModSettingValue {
    fn load<R>(mut reader: R, setting_type: u32) -> anyhow::Result<ModSettingValue>
    where
        R: Read,
    {
        match setting_type {
            0 => Ok(ModSettingValue::None),
            1 => match reader.read_be::<u32>().context("Reading bool value")? {
                0 => Ok(ModSettingValue::Bool(false)),
                1 => Ok(ModSettingValue::Bool(true)),
                2.. => Err(anyhow!("Illegal bool value")),
            },
            2 => Ok(ModSettingValue::Number(
                reader.read_be().context("Reading number value")?,
            )),
            3 => {
                let size = reader.read_be::<u32>().context("Reading string length")?;
                let mut buf = vec![0; size as usize];
                reader.read_exact(&mut buf).context("Reading string data")?;
                Ok(ModSettingValue::String(
                    String::from_utf8(buf.clone()).context(
                        // TODO: another wasteful clone
                        format!("Converting string data {:?} to utf8", buf),
                    )?,
                ))
            }
            4.. => Err(anyhow!("Illegal setting type {setting_type}")),
        }
    }

    fn type_int(&self) -> u32 {
        match self {
            ModSettingValue::None => 0,
            ModSettingValue::Bool(_) => 1,
            ModSettingValue::Number(_) => 2,
            ModSettingValue::String(_) => 3,
        }
    }

    /// Note that you need to save the type yourself via [`type_int`], as it is seperated from the value
    fn save<W>(&self, mut writer: W) -> anyhow::Result<()>
    where
        W: Write,
    {
        match self {
            ModSettingValue::None => Ok(()),
            ModSettingValue::Bool(v) => writer
                .write_be::<u32>(if *v { 1 } else { 0 })
                .context(format!("Writing bool value {v}")),
            ModSettingValue::Number(v) => writer
                .write_be::<f64>(*v)
                .context(format!("Writing number value {v}")),
            ModSettingValue::String(v) => {
                let len = v.len();
                (|| {
                    writer
                        .write_be::<u32>(len as u32)
                        .context(format!("Writing string length {len}"))?;
                    let key_buf = v.as_bytes();
                    writer.write_all(key_buf).context("Writing string value")?;
                    Ok::<_, Error>(())
                })()
                .context(format!("Writing string {v}"))
            }
        }
    }
}

impl ModPack {
    fn load_v0<R>(mut reader: R) -> anyhow::Result<ModPack>
    where
        R: Read,
    {
        let name_len = reader
            .read_le::<usize>()
            .context("Reading modpack name length")?;
        let mut name_buf = vec![0; name_len];
        reader
            .read_exact(&mut name_buf)
            .context("Reading modpack name")?;
        let name = String::from_utf8(name_buf).context("Converting modpack name to utf8")?;
        let err_name = name.clone();
        (|| {
            let num_mods = reader
                .read_le::<usize>()
                .context(format!("Reading modpack number of mods"))?;

            let mut mods = Vec::with_capacity(num_mods);

            for _ in 0..num_mods {
                let mod_name_len = reader
                    .read_le::<usize>()
                    .context(format!("Reading mod name length"))?;
                let mut mod_name_buf = vec![0; mod_name_len];
                reader
                    .read_exact(&mut mod_name_buf)
                    .context("Reading mod name")?;
                let mod_name =
                    String::from_utf8(mod_name_buf).context("Converting mod name to utf8")?;
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
                name,
                mods,
                settings: ModSettings(settings),
            })
        })()
        .context(format!("Loading pack {err_name}"))
    }

    pub fn apply(&self, mod_list_config: &mut ModListConfig) {
        let mut enabled_set = HashSet::new();
        for nmod in &self.mods {
            enabled_set.insert(nmod);
        }
        mod_list_config.mods.iter_mut().for_each(|e| {
            if let ModKind::Normal(nmod) = &mut e.kind {
                nmod.enabled = enabled_set.contains(&e.id);
            }
        });
        for (key, values) in self.settings.0.iter() {
            mod_list_config
                .mod_settings
                .0
                .insert(key.clone(), values.clone());
        }
    }

    pub fn load<R>(mut reader: R) -> anyhow::Result<ModPack>
    where
        R: Read,
    {
        let version = reader
            .read_le::<usize>()
            .context("Reading modpack schema version")?;
        match version {
            0 => Self::load_v0(reader),
            1.. => bail!("Attempted to load future modpack schema (v{version})"),
        }
    }

    pub fn save<W>(&self, mut writer: W) -> anyhow::Result<()>
    where
        W: Write,
    {
        (|| {
            writer
                .write_le::<usize>(0)
                .context("Writing modpack schema version")?;
            writer
                .write_le::<usize>(self.name.len())
                .context("Writing modpack name length")?;
            writer
                .write_all(self.name.as_bytes())
                .context("Writing modpack name")?;
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
                .write_le::<usize>(self.settings.0.len())
                .context("Writing modpack number of settings")?;

            for (key, values) in &self.settings.0 {
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
    pub fn render(
        &self,
        ui: &mut Ui,
        mod_list: &mut ModListConfig,
        installed: &HashSet<String>,
    ) -> Option<String> {
        ui.horizontal(|ui| {
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

            // on_hover_ui is lazy so we can't set the error in it
            ui.label(&self.name).on_hover_ui(|ui| {
                for nmod in self.mods.iter() {
                    ui.fixed_size_group(40.0, |ui| {
                        if !installed.contains(nmod) {
                            ui.label(format!("{UNSAFE}"))
                                .on_hover_text("Missing this mod");
                        }
                    });
                    ui.label(nmod);
                }
            });

            if ui.button("Apply").clicked() {
                self.apply(mod_list);
                if let Some(err) = error {
                    Some("Missing mods:\n".to_owned() + &err)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .inner
    }

    pub fn new(name: &str, mods: &[String], settings: &ModSettings) -> ModPack {
        ModPack {
            name: name.to_owned(),
            mods: mods.to_vec(),
            settings: settings.clone(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ModSetting {
    pub fn load<R>(mut reader: R) -> anyhow::Result<ModSetting>
    where
        R: Read,
    {
        let key_len = reader.read_be::<u32>().context("Reading key length")?;
        let mut buf = vec![0; key_len as usize];
        reader.read_exact(&mut buf).context("Reading key")?;
        let key = String::from_utf8(buf.clone()) // TODO: remove wasteful clone!
            .context(format!("Converting key {:?} to utf8", buf))?;
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

    pub fn save<W>(&self, mut writer: W) -> anyhow::Result<()>
    where
        W: Write,
    {
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
    pub fn empty() -> ModSettings {
        ModSettings(HashMap::new())
    }
    // basically a port of dexters https://github.com/dextercd/NoitaSettings/blob/main/settings_main.cpp
    pub fn load<R>(reader: R, file_size: usize) -> anyhow::Result<ModSettings>
    where
        R: Read,
    {
        let mut settings = HashMap::new();
        let mut decompressed =
            ByteVec(decompress_file(reader, file_size).context("Decompressing file")?);
        // File::create("./mod_settings")
        //     .unwrap()
        //     .write_all(&decompressed.0)
        //     .unwrap();
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
        Ok(ModSettings(settings))
    }

    pub fn save<W>(&self, writer: W) -> anyhow::Result<()>
    where
        W: Write,
    {
        let mut buf = ByteVec(Vec::new());
        buf.write_be::<u64>(self.0.len() as u64)
            .context("Writing number of settings")?;
        for (key, values) in self.0.iter() {
            let setting = ModSetting {
                key: key.clone(),
                values: values.clone(),
            };
            setting.save(&mut buf)?; // TODO: remove clones
        }
        compress_file(writer, &buf.0).context("Compressing to file")
    }
}
