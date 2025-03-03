use std::{
    fmt::{Debug, Display},
    io::{Read, Write},
};

use anyhow::{anyhow, Context};
use bytemuck::{AnyBitPattern, NoUninit};

pub enum Endianness {
    Little,
    Big,
}

pub trait ByteReaderExt {
    fn read_le<T: AnyBitPattern>(&mut self) -> anyhow::Result<T>;
    fn read_be<T: AnyBitPattern>(&mut self) -> anyhow::Result<T>;
    fn read_str<T: AnyBitPattern>(&mut self, endianness: Endianness) -> anyhow::Result<String>
    where
        <T as TryInto<usize>>::Error: Debug,
        usize: TryFrom<T>;
}

impl<R: Read> ByteReaderExt for R {
    fn read_le<T: AnyBitPattern>(&mut self) -> anyhow::Result<T> {
        let mut buffer = vec![0; size_of::<T>()];
        self.read_exact(&mut buffer)
            .context(format!("Reading {} bytes into buffer", buffer.capacity()))?;
        Ok(*(bytemuck::try_from_bytes::<T>(&buffer)
            .map_err(|e| anyhow!("Failed to try from bytes {e}"))?))
    }

    fn read_str<T: AnyBitPattern>(&mut self, endianness: Endianness) -> anyhow::Result<String>
    where
        <T as TryInto<usize>>::Error: Debug,
        T: TryInto<usize>,
    {
        let len: usize = match endianness {
            Endianness::Little => self
                .read_le::<T>()
                .context("Reading string length")?
                .try_into()
                .expect("Length must be able to be cast to usize"), // TODO: make this anyhow / comptime ideally
            Endianness::Big => self
                .read_be::<T>()
                .context("Reading string length")?
                .try_into()
                .expect("Length must be able to be cast to usize"),
        };

        let mut buf = vec![0; len as usize];
        self.read_exact(&mut buf).context("Reading string data")?;
        Ok(String::from_utf8(buf.clone()).context(
            // TODO: another wasteful clone
            format!("Converting string data {:?} to utf8", buf),
        )?)
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
    fn write_str<T: NoUninit>(&mut self, value: &str, endianness: Endianness) -> anyhow::Result<()>
    where
        <usize as TryInto<T>>::Error: Debug,
        T: Display,
        T: TryFrom<usize>;
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

    fn write_str<T: NoUninit>(&mut self, value: &str, endianness: Endianness) -> anyhow::Result<()>
    where
        <usize as TryInto<T>>::Error: Debug,
        T: Display,
        T: TryFrom<usize>,
    {
        let len: T = value
            .len()
            .try_into()
            .expect("Length should be able to cast to T");
        match endianness {
            Endianness::Little => self.write_le(len),
            Endianness::Big => self.write_be(len),
        }
        .context(format!("Writing string length {len}"))?;
        self.write_all(value.as_bytes())
            .context("Writing string body")?;
        Ok(())
    }
}
