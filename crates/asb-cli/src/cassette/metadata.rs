//! A bounded metadata view prevents ZIP fallback parsing unapproved directories.
//!
//! The ZIP crate may search earlier EOCDs after a parse error and follows header
//! lengths rather than enforcing the EOCD directory-byte bound. Before creating
//! its index we therefore walk a bounded directory snapshot, and expose only that
//! snapshot and its chosen EOCD. All other construction reads synthesize zeros.
//! Payload access is enabled only after checking the index's offset and count.
use super::{Error, Limits};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct MetadataReader {
    file: File,
    phase: Arc<AtomicBool>,
    position: u64,
    length: u64,
    start: u64,
    metadata: Vec<u8>,
}

impl MetadataReader {
    pub(super) fn new(
        mut file: File,
        limits: Limits,
    ) -> Result<(Self, Arc<AtomicBool>, usize, u64), Error> {
        let length = file.metadata().map_err(error)?.len();
        if length < 22 || length > limits.max_archive_bytes {
            return Err(Error(
                "cassette archive length is invalid or exceeds limit".into(),
            ));
        }
        let tail_len = length.min(65_557);
        file.seek(SeekFrom::Start(length - tail_len))
            .map_err(error)?;
        let mut tail = vec![0; tail_len as usize];
        file.read_exact(&mut tail).map_err(error)?;
        let offset = (0..=tail.len() - 22)
            .rev()
            .find(|index| {
                &tail[*index..*index + 4] == b"PK\x05\x06"
                    && *index + 22 + word(&tail[*index..], 20) as usize == tail.len()
            })
            .ok_or_else(|| Error("cassette ZIP directory is missing or truncated".into()))?;
        let end = &tail[offset..];
        let entries = word(end, 10) as usize;
        let bytes = dword(end, 12);
        let start = dword(end, 16);
        let eocd_offset = length - tail_len + offset as u64;
        if word(end, 4) != 0
            || word(end, 6) != 0
            || word(end, 8) as usize != entries
            || entries == 0
            || entries == u16::MAX as usize
            || entries > limits.max_tracks.saturating_add(1)
            || bytes > limits.max_directory_bytes
            || bytes == u32::MAX as u64
            || start == u32::MAX as u64
            || start.checked_add(bytes) != Some(eocd_offset)
        {
            return Err(Error("unsupported or excessive cassette ZIP directory (requires bounded single-disk ZIP32)".into()));
        }
        file.seek(SeekFrom::Start(start)).map_err(error)?;
        let count =
            usize::try_from(bytes).map_err(|_| Error("cassette directory size overflow".into()))?;
        let mut metadata = vec![0; count];
        file.read_exact(&mut metadata).map_err(error)?;
        validate_directory(&metadata, entries, start)?;
        metadata.extend_from_slice(end);
        let phase = Arc::new(AtomicBool::new(true));
        Ok((
            Self {
                file,
                phase: phase.clone(),
                position: 0,
                length,
                start,
                metadata,
            },
            phase,
            entries,
            start,
        ))
    }
}

fn validate_directory(bytes: &[u8], count: usize, start: u64) -> Result<(), Error> {
    let bad = || {
        Error("cassette central-directory headers exceed or disagree with approved bounds".into())
    };
    let mut cursor = 0usize;
    for _ in 0..count {
        let header = bytes
            .get(cursor..cursor.checked_add(46).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        if &header[..4] != b"PK\x01\x02"
            || word(header, 34) != 0
            || [20, 24, 42]
                .iter()
                .any(|index| dword(header, *index) == u32::MAX as u64)
            || dword(header, 42) >= start
        {
            return Err(bad());
        }
        let name_len = word(header, 28) as usize;
        let extra_len = word(header, 30) as usize;
        let comment_len = word(header, 32) as usize;
        let extra_start = cursor
            .checked_add(46)
            .and_then(|value| value.checked_add(name_len))
            .ok_or_else(bad)?;
        let extra_end = extra_start.checked_add(extra_len).ok_or_else(bad)?;
        let extra = bytes.get(extra_start..extra_end).ok_or_else(bad)?;
        let mut field = 0usize;
        while field < extra.len() {
            let info = extra
                .get(field..field.checked_add(4).ok_or_else(bad)?)
                .ok_or_else(bad)?;
            if word(info, 0) == 1 {
                return Err(bad());
            } // ZIP64 extra field
            field = field
                .checked_add(4 + word(info, 2) as usize)
                .filter(|end| *end <= extra.len())
                .ok_or_else(bad)?;
        }
        cursor = extra_end
            .checked_add(comment_len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(bad)?;
    }
    if cursor != bytes.len() {
        return Err(bad());
    }
    Ok(())
}

impl Read for MetadataReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if !self.phase.load(Ordering::Acquire) {
            self.file.seek(SeekFrom::Start(self.position))?;
            let count = self.file.read(output)?;
            self.position += count as u64;
            return Ok(count);
        }
        let count = self
            .length
            .saturating_sub(self.position)
            .min(output.len() as u64) as usize;
        output[..count].fill(0);
        let begin = self.position.max(self.start);
        let end = (self.position + count as u64).min(self.length);
        if begin < end {
            let destination = (begin - self.position) as usize;
            let source = (begin - self.start) as usize;
            let length = (end - begin) as usize;
            output[destination..destination + length]
                .copy_from_slice(&self.metadata[source..source + length]);
        }
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for MetadataReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.length) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        self.position = u64::try_from(position)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cassette seek"))?;
        Ok(self.position)
    }
}

fn word(bytes: &[u8], index: usize) -> u16 {
    u16::from_le_bytes([bytes[index], bytes[index + 1]])
}
fn dword(bytes: &[u8], index: usize) -> u64 {
    u32::from_le_bytes(
        bytes[index..index + 4]
            .try_into()
            .expect("bounded ZIP field"),
    ) as u64
}
fn error(error: io::Error) -> Error {
    Error(format!("cassette metadata I/O error: {error}"))
}
