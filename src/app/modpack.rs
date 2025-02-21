use std::{
    cmp::min,
    collections::HashMap,
    fs::File,
    io::{Read, Write},
};

use anyhow::{anyhow, bail, Context};
use bytemuck::{AnyBitPattern, Pod};
use fastlz;

#[derive(Clone, Debug)]
enum ModSetting {
    None,
    Bool(bool),
    Number(f64),
    String(String),
}

#[derive(Clone, Debug)]
struct ModSettingPair {
    current: ModSetting,
    next: ModSetting,
}

#[derive(Clone, Debug)]
pub struct ModSettings(HashMap<String, ModSettingPair>);

#[derive(Clone, Debug)]
pub struct ModPack {
    mods: Vec<String>,
    settings: ModSettings,
}

trait ByteReaderExt {
    fn read_le<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern;
    fn read_be<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern;
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

impl<R> ByteReaderExt for R
where
    R: Read,
{
    fn read_le<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern,
    {
        let mut buffer = vec![0; size_of::<T>()];
        self.read_exact(&mut buffer)
            .context(format!("Reading {} bytes into buffer", buffer.capacity()))?;
        Ok(*(bytemuck::try_from_bytes::<T>(&buffer)
            .map_err(|e| anyhow!("Failed to try from bytes {e}"))?))
    }

    fn read_be<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern,
    {
        let mut buffer = vec![0; size_of::<T>()];
        self.read_exact(&mut buffer)
            .context(format!("Reading {} bytes into buffer", buffer.capacity()))?;
        buffer.reverse();
        Ok(*(bytemuck::try_from_bytes::<T>(&buffer)
            .map_err(|e| anyhow!("Failed to try from bytes {e}"))?))
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

impl ModSetting {
    pub fn load<R>(mut reader: R, setting_type: u32) -> anyhow::Result<ModSetting>
    where
        R: Read,
    {
        match setting_type {
            0 => Ok(ModSetting::None),
            1 => match reader.read_be::<u32>().context("Reading bool value")? {
                0 => Ok(ModSetting::Bool(false)),
                1 => Ok(ModSetting::Bool(true)),
                2.. => Err(anyhow!("Illegaal bool value")),
            },
            2 => Ok(ModSetting::Number(
                reader.read_be().context("Reading number value")?,
            )),
            3 => {
                let size = reader.read_be::<u32>().context("Reading string length")?;
                let mut buf = vec![0; size as usize];
                reader.read_exact(&mut buf).context("Reading string data")?;
                Ok(ModSetting::String(String::from_utf8(buf.clone()).context(
                    // TODO: another wasteful clone
                    format!("Converting string data {:?} to utf8", buf),
                )?))
            }
            4.. => Err(anyhow!("Illegal setting type {setting_type}")),
        }
    }
}

impl ModPack {}
impl ModSettings {
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
            num_entries += 1;
            let key_len = decompressed
                .read_be::<u32>()
                .context("Reading key length")?;
            let mut buf = vec![0; key_len as usize];
            decompressed.read_exact(&mut buf).context("Reading key")?;
            let key = String::from_utf8(buf.clone()) // TODO: remove wasteful clone!
                .context(format!("Converting key {:?} to utf8", buf))?;
            let setting_current_type = decompressed
                .read_be::<u32>()
                .context(format!("Reading setting {key} current type"))?;
            let setting_next_type = decompressed
                .read_be::<u32>()
                .context(format!("Reading setting {key} next type"))?;
            let setting_current = ModSetting::load(&mut decompressed, setting_current_type)
                .context(format!("Reading setting {key} current value"))?;
            let setting_next = ModSetting::load(&mut decompressed, setting_next_type)
                .context(format!("Reading setting {key} next value"))?;
            settings.insert(
                key,
                ModSettingPair {
                    current: setting_current,
                    next: setting_next,
                },
            );
        }
        if num_entries != expected_num_entries {
            bail!("Expected {expected_num_entries} but there were {num_entries}");
        }
        Ok(ModSettings(settings))
    }
}
