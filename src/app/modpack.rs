use std::{cmp::min, collections::HashMap, io::Read};

use anyhow::{anyhow, bail, Context};
use bytemuck::{AnyBitPattern, Pod};
use fastlz;

#[derive(Clone, Debug)]
enum ModSetting {
    None(),
    Bool(bool),
    Number(f64),
    String(String),
}

#[derive(Clone, Debug)]
pub struct ModSettings(HashMap<String, ModSetting>);

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

struct ByteVec(Vec<u8>);

impl Read for ByteVec {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = min(buf.len(), self.0.len());
        buf[..len].copy_from_slice(&self.0[..len]);
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
    let mut output = Vec::with_capacity(decompressed_size as usize);
    fastlz::decompress(&compressed, &mut output)
        .map_err(|_| anyhow!("FastLZ failed to decompress"))?;
    Ok(output)
}

impl ModPack {}
impl ModSettings {
    pub fn load<R>(reader: R, file_size: usize) -> anyhow::Result<ModSettings>
    where
        R: Read,
    {
        let mut decompressed =
            ByteVec(decompress_file(reader, file_size).context("Decompressing file")?);
        decompressed.read_be::<u32>();
        Ok(ModSettings(HashMap::new()))
    }
}
