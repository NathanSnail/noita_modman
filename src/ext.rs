use std::io::{Read, Write};

use anyhow::{anyhow, Context};
use bytemuck::{AnyBitPattern, NoUninit};

pub trait ByteReaderExt {
    fn read_le<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern;
    fn read_be<T>(&mut self) -> anyhow::Result<T>
    where
        T: AnyBitPattern;
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

pub trait ByteWriterExt {
    fn write_le<T>(&mut self, value: T) -> anyhow::Result<()>
    where
        T: NoUninit;
    fn write_be<T>(&mut self, value: T) -> anyhow::Result<()>
    where
        T: NoUninit;
}

impl<W> ByteWriterExt for W
where
    W: Write,
{
    fn write_le<T>(&mut self, value: T) -> anyhow::Result<()>
    where
        T: NoUninit,
    {
        let bytes = bytemuck::bytes_of(&value);
        self.write_all(bytes)
            .context(format!("Writing {bytes:?} to the buffer"))?;
        Ok(())
    }

    fn write_be<T>(&mut self, value: T) -> anyhow::Result<()>
    where
        T: NoUninit,
    {
        let bytes = bytemuck::bytes_of(&value);
        let mut bytes_mut = bytes.to_vec();
        bytes_mut.reverse();
        self.write_all(&bytes_mut)
            .context(format!("Writing {bytes:?} to the buffer"))?;
        Ok(())
    }
}
