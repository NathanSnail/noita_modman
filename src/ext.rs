use std::io::{Read, Write};

use anyhow::{anyhow, Context};
use bytemuck::{AnyBitPattern, NoUninit};

pub trait ByteReaderExt {
    fn read_le<T: AnyBitPattern>(&mut self) -> anyhow::Result<T>;
    fn read_be<T: AnyBitPattern>(&mut self) -> anyhow::Result<T>;
}

impl<R: Read> ByteReaderExt for R {
    fn read_le<T: AnyBitPattern>(&mut self) -> anyhow::Result<T> {
        let mut buffer = vec![0; size_of::<T>()];
        self.read_exact(&mut buffer)
            .context(format!("Reading {} bytes into buffer", buffer.capacity()))?;
        Ok(*(bytemuck::try_from_bytes::<T>(&buffer)
            .map_err(|e| anyhow!("Failed to try from bytes {e}"))?))
    }

    fn read_be<T: AnyBitPattern>(&mut self) -> anyhow::Result<T> {
        let mut buffer = vec![0; size_of::<T>()];
        self.read_exact(&mut buffer)
            .context(format!("Reading {} bytes into buffer", buffer.capacity()))?;
        buffer.reverse();
        Ok(*(bytemuck::try_from_bytes::<T>(&buffer)
            .map_err(|e| anyhow!("Failed to try from bytes {e}"))?))
    }
}

pub trait ByteWriterExt {
    fn write_le<T: NoUninit>(&mut self, value: T) -> anyhow::Result<()>;
    fn write_be<T: NoUninit>(&mut self, value: T) -> anyhow::Result<()>;
}

impl<W: Write> ByteWriterExt for W {
    fn write_le<T: NoUninit>(&mut self, value: T) -> anyhow::Result<()> {
        let bytes = bytemuck::bytes_of(&value);
        self.write_all(bytes)
            .context(format!("Writing {bytes:?} to the buffer"))?;
        Ok(())
    }

    fn write_be<T: NoUninit>(&mut self, value: T) -> anyhow::Result<()> {
        let bytes = bytemuck::bytes_of(&value);
        let mut bytes_mut = bytes.to_vec();
        bytes_mut.reverse();
        self.write_all(&bytes_mut)
            .context(format!("Writing {bytes:?} to the buffer"))?;
        Ok(())
    }
}
