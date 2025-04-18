use quickcheck::{Arbitrary, Gen};
use std::{
    collections::HashMap,
    io::{Read, Write},
    iter::{empty, zip},
};

use anyhow::{anyhow, Context};
use egui::Ui;

use crate::ext::{ByteReaderExt, ByteWriterExt, Endianness::Big};

#[derive(Clone, Debug, PartialEq)]
pub enum ModSettingValue {
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
    pub current: ModSettingValue,
    pub next: ModSettingValue,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModSettings {
    pub values: HashMap<String, ModSettingPair>,
    pub grouped: super::ModSettingsGroup,
}

impl ModSettingPair {
    pub fn render(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Current");
            self.current.render(ui)
        });
        ui.horizontal(|ui| {
            ui.label("Next");
            self.next.render(ui)
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModSetting {
    pub key: String,
    pub values: ModSettingPair,
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
    }
}

impl ModSettingValue {
    pub fn load<R: Read>(mut reader: R, setting_type: u32) -> anyhow::Result<ModSettingValue> {
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

    pub fn type_int(&self) -> u32 {
        match self {
            ModSettingValue::None => 0,
            ModSettingValue::Bool(_) => 1,
            ModSettingValue::Number(_) => 2,
            ModSettingValue::String(_) => 3,
        }
    }

    /// Note that you need to save the type yourself via [`type_int`], as it is seperated from the value
    pub fn save<W: Write>(&self, mut writer: W) -> anyhow::Result<()> {
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

    pub fn render(&self, ui: &mut Ui) {
        match self {
            ModSettingValue::None => ui.code("None()"),
            ModSettingValue::Bool(val) => ui.code(format!("Bool({val})")),
            ModSettingValue::Number(val) => ui.code(format!("Number({val})")),
            ModSettingValue::String(val) => ui.code(format!("String(\"{val}\")")),
        };
    }
}

impl Arbitrary for ModSettings {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut settings = Self {
            values: HashMap::new(),
            ..Default::default()
        };
        for _ in 0..(u32::arbitrary(g) % 100) {
            settings
                .values
                .insert(String::arbitrary(g), ModSettingPair::arbitrary(g));
        }
        settings
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.values.shrink().map(|hash_map| Self {
            values: hash_map,
            ..Default::default()
        }))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use anyhow::{anyhow, Error};

    use crate::{app::modpack::decompress_file, ext::ByteVec};

    use super::{super::compress_file, ModSettingPair, ModSettingValue, ModSettings};

    #[quickcheck]
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

    #[quickcheck]
    fn save_load_buffer(value: String) -> bool {
        let bytes = value.as_bytes();
        let mut buffer = ByteVec(Vec::new());
        compress_file(&mut buffer, bytes).expect("Saving errored");
        let len = buffer.0.len();
        bytes == decompress_file(&mut buffer, len).expect("Loading errored")
    }

    #[test]
    fn settings() {
        let mut map = HashMap::new();
        map.insert(
            "\0\0\u{1}.K\u{2000}𐀀\u{80}ࠀ\0𐁀\0\0\u{80}\0\u{1}\u{1}ࠁ\u{2}".to_string(),
            ModSettingPair {
                current: ModSettingValue::Bool(false),
                next: ModSettingValue::Bool(false),
            },
        );
        let mut buffer = ByteVec(Vec::new());
        ModSettings {
            values: map,
            ..Default::default()
        }
        .save(&mut buffer)
        .expect("Saving must work");
        let len = buffer.0.len();
        ModSettings::load(&mut buffer, len).expect("Loading must work");
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
            ModSettingValue::None => Box::new(empty()),
            ModSettingValue::Bool(v) => Box::new(v.shrink().map(ModSettingValue::Bool)),
            ModSettingValue::Number(v) => Box::new(v.shrink().map(ModSettingValue::Number)),
            ModSettingValue::String(v) => Box::new(v.shrink().map(ModSettingValue::String)),
        }
    }
}
