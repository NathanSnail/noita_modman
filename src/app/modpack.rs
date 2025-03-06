use quickcheck::{Arbitrary, Gen};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    iter::{self, zip},
};

use anyhow::{anyhow, bail, Context, Error};
use egui::Ui;
use fastlz;

use crate::{
    app::{ModListConfig, UiSizedExt},
    ext::{
        ByteReaderExt, ByteVec, ByteWriterExt,
        Endianness::{Big, Little},
    },
    icons::UNSAFE,
    r#mod::ModKind,
};

#[derive(Clone, Debug, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub struct ModSettingPair {
    current: ModSettingValue,
    next: ModSettingValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModSetting {
    key: String,
    values: ModSettingPair,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModSettings(HashMap<String, ModSettingPair>);

#[derive(Clone, Debug)]
pub struct ModPack {
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

fn compress_file<W: Write>(mut writer: W, buf: &[u8]) -> anyhow::Result<()> {
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
    fn load<R: Read>(mut reader: R, setting_type: u32) -> anyhow::Result<ModSettingValue> {
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
            3 => Ok(ModSettingValue::String(reader.read_str::<u32>(Big)?)),
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
    fn save<W: Write>(&self, mut writer: W) -> anyhow::Result<()> {
        match self {
            ModSettingValue::None => Ok(()),
            ModSettingValue::Bool(v) => writer
                .write_be::<u32>(if *v { 1 } else { 0 })
                .context(format!("Writing bool value {v}")),
            ModSettingValue::Number(v) => writer
                .write_be::<f64>(*v)
                .context(format!("Writing number value {v}")),
            ModSettingValue::String(v) => writer
                .write_str::<u32>(v, Big)
                .context(format!("Writing string {v}")),
        }
    }
}

impl ModPack {
    fn load_v0<R: Read>(mut reader: R) -> anyhow::Result<ModPack> {
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
                name,
                mods,
                settings: ModSettings(settings),
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
        mod_list_config
            .mods
            .iter_mut()
            .enumerate()
            .for_each(|(i, e)| {
                if let ModKind::Normal(nmod) = &mut e.kind {
                    if let Some(v) = enabled.get(&e.id) {
                        nmod.enabled = true;
                        enabled_mods.push((e.clone(), *v));
                        enabled_idxs.push(i);
                    } else {
                        nmod.enabled = false;
                    }
                }
            });

        enabled_mods.sort_by_key(|e| e.1);
        for (nmod, idx) in zip(enabled_mods, enabled_idxs) {
            mod_list_config.mods[idx] = nmod.0;
        }

        for (key, values) in self.settings.0.iter() {
            mod_list_config
                .mod_settings
                .0
                .insert(key.clone(), values.clone());
        }
    }

    pub fn load<R: Read>(mut reader: R) -> anyhow::Result<ModPack> {
        let version = reader
            .read_le::<usize>()
            .context("Reading modpack schema version")?;
        match version {
            0 => Self::load_v0(reader),
            1.. => bail!("Attempted to load future modpack schema (v{version})"),
        }
    }

    pub fn save<W: Write>(&self, mut writer: W) -> anyhow::Result<()> {
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
        search_term: &mut String,
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
                *search_term = self.name.clone();
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

impl ModSettingValue {
    pub fn new_none() -> Self {
        ModSettingValue::None
    }

    pub fn new_bool(v: bool) -> Self {
        ModSettingValue::Bool(v)
    }

    pub fn new_number(v: f64) -> Self {
        ModSettingValue::Number(v)
    }

    pub fn new_string(v: String) -> Self {
        ModSettingValue::String(v)
    }
}
impl ModSettingPair {
    pub fn new(current: ModSettingValue, next: ModSettingValue) -> Self {
        Self { current, next }
    }
}

impl ModSetting {
    pub fn new(key: String, values: ModSettingPair) -> Self {
        Self { key, values }
    }

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
    pub fn empty() -> ModSettings {
        ModSettings(HashMap::new())
    }
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
        Ok(ModSettings(settings))
    }

    pub fn save<W: Write>(&self, writer: W) -> anyhow::Result<()> {
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

#[derive(Copy, Clone, PartialEq)]
struct NonNaNF64(f64); // NaN is stupid and isn't equal to itself, so don't test for it

impl Arbitrary for NonNaNF64 {
    fn arbitrary(g: &mut Gen) -> Self {
        loop {
            let value = f64::arbitrary(g);
            if !value.is_nan() {
                return NonNaNF64(value);
            }
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().filter(|v| !v.is_nan()).map(NonNaNF64))
    }
}

impl Arbitrary for ModSettingValue {
    fn arbitrary(g: &mut Gen) -> Self {
        let n = u32::arbitrary(g);
        match n % 4 {
            0 => Self::None,
            1 => Self::Bool(bool::arbitrary(g)),
            2 => Self::Number(NonNaNF64::arbitrary(g).0),
            3 => Self::String(String::arbitrary(g)),
            _ => unreachable!(),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self {
            ModSettingValue::None => Box::new(iter::empty()),
            ModSettingValue::Bool(v) => Box::new(v.shrink().map(ModSettingValue::Bool)),
            ModSettingValue::Number(v) => Box::new(v.shrink().map(ModSettingValue::Number)),
            ModSettingValue::String(v) => Box::new(v.shrink().map(ModSettingValue::String)),
        }
    }
}

impl Arbitrary for ModSettingPair {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            current: ModSettingValue::arbitrary(g),
            next: ModSettingValue::arbitrary(g),
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            zip(self.next.shrink(), self.current.shrink()) // not ideal but i cannot figure out how to do the proper nested approach
                .map(|(next, current)| Self { next, current }),
        )
        // Box::new(self.current.shrink().flat_map(move |current| {
        //     self.next.shrink().map(move |next| Self {
        //         current: current.clone(),
        //         next: next.clone(),
        //     })
        // }))
    }
}

impl Arbitrary for ModSettings {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut settings = Self(HashMap::new());
        for _ in 0..(u32::arbitrary(g) % 100) {
            settings
                .0
                .insert(String::arbitrary(g), ModSettingPair::arbitrary(g));
        }
        settings
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(Self))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::{compress_file, decompress_file};
    use super::{ModSettingValue, ModSettings};
    use crate::app::modpack::ModSettingPair;
    use crate::ext::ByteVec;
    use anyhow::{anyhow, Error};

    #[quickcheck(props = 10000)]
    fn save_load_settings(value: ModSettings) -> bool {
        let mut buffer = ByteVec(Vec::new());
        value.save(&mut buffer).expect("Saving errored");
        let len = buffer.0.len();
        let loaded = ModSettings::load(&mut buffer, len).expect("Loading errored");
        if value != loaded {
            Err::<(), Error>(anyhow!("{buffer:?}")).unwrap();
        }
        true
    }

    #[quickcheck(props = 10000)]
    fn save_load_buffer(value: String) -> bool {
        let bytes = value.as_bytes();
        let mut buffer = ByteVec(Vec::new());
        compress_file(&mut buffer, bytes).expect("Saving errored");
        let len = buffer.0.len();
        bytes == decompress_file(&mut buffer, len).expect("Loading errored")
    }

    // TODO: this test fails for some reason
    #[quickcheck(props = 1)]
    fn settings(_: bool) {
        let mut map = HashMap::new();
        map.insert(
            "\0\0\u{1}.K\u{2000}𐀀\u{80}ࠀ\0𐁀\0\0\u{80}\0\u{1}\u{1}ࠁ\u{2}".to_string(),
            ModSettingPair::new(
                ModSettingValue::new_bool(false),
                ModSettingValue::new_bool(false),
            ),
        );
        let mut buffer = ByteVec(Vec::new());
        ModSettings(map)
            .save(&mut buffer)
            .expect("Saving must work");
        let len = buffer.0.len();
        ModSettings::load(&mut buffer, len).expect("Loading must work");
    }

    #[quickcheck(props = 1)]
    fn compress(_: bool) -> bool {
        let s = "\u{fff4}\u{2000}\u{fff4}⁀ࠀ\0\0\0\0".to_owned();
        let mut buffer = ByteVec(Vec::new());
        compress_file(&mut buffer, s.as_bytes()).expect("Saving must work");
        let len = buffer.0.len();
        dbg!(s.as_bytes());
        let decompressed = decompress_file(&mut buffer, len).expect("Loading must work");
        dbg!(&decompressed);
        s.as_bytes() == decompressed
    }
}
