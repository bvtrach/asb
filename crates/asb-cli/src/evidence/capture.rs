//! Opened raw source custody. Consumption follows confirmed result publication.
use super::*;

pub(crate) struct Capture {
    path: PathBuf,
    file: File,
    #[cfg(windows)]
    reader_file: File,
    metadata: Metadata,
    stopped: Option<Metadata>,
}
impl Capture {
    /// Establish identity before any child/worker receives its output handle.
    /// Existing sources are never silently truncated/re-adopted as this attempt.
    pub(crate) fn create(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(|error| {
            format!(
                "cannot create owned raw capture {}: {error}; preserve existing capture evidence",
                path.display()
            )
        })?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        #[cfg(windows)]
        let reader_file = independent_windows_reader(&file).map_err(|error| format!(
            "cannot establish independent admitted capture reader {}: {error}; no producer was admitted", path.display()))?;
        Ok(Self {
            path: path.into(),
            file,
            #[cfg(windows)]
            reader_file,
            metadata,
            stopped: None,
        })
    }
    pub(crate) fn writer(&self) -> Result<File, String> {
        self.file.try_clone().map_err(|error| {
            format!(
                "cannot prepare raw capture writer {}: {error}",
                self.path.display()
            )
        })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn freeze(&mut self) -> Result<(), String> {
        self.stopped = Some(self.file.metadata().map_err(|error| {
            format!(
                "cannot freeze admitted raw source {}: {error}",
                self.path.display()
            )
        })?);
        Ok(())
    }
    pub(crate) fn reader(&self) -> Result<CaptureReader<'_>, String> {
        let length = self
            .stopped
            .as_ref()
            .ok_or("raw capture has not reached its stopped boundary")?
            .len();
        Ok(CaptureReader {
            file: self.read_source(),
            offset: 0,
            length,
        })
    }
    /// Startup acknowledgements also come from the admitted handle, while its
    /// producer is still running. This prefix is not a complete result reader.
    pub(crate) fn startup_diagnostic(&self, limit: usize) -> Result<String, String> {
        let length = self
            .file
            .metadata()
            .map_err(|error| error.to_string())?
            .len();
        let mut bytes = Vec::new();
        CaptureReader {
            file: self.read_source(),
            offset: 0,
            length,
        }
        .take(limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    pub(crate) fn verify_stopped(&self) -> Result<(), String> {
        let previous = self
            .stopped
            .as_ref()
            .ok_or("raw capture has not reached its stopped boundary")?;
        let now = self.file.metadata().map_err(|error| error.to_string())?;
        if previous.len() != now.len() || previous.modified().ok() != now.modified().ok() {
            return Err(format!(
                "admitted raw source {} changed after stop; no complete measurements or cleanup confirmed",
                self.path.display()
            ));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let file = open_regular(path)
            .map_err(|error| format!("cannot retain raw capture {}: {error}", path.display()))?
            .ok_or_else(|| {
                format!(
                    "raw capture {} is missing; cleanup ownership unavailable",
                    path.display()
                )
            })?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("cannot inspect raw capture {}: {error}", path.display()))?;
        #[cfg(windows)]
        let reader_file = independent_windows_reader(&file).map_err(|error| error.to_string())?;
        Ok(Self {
            path: path.into(),
            file,
            #[cfg(windows)]
            reader_file,
            stopped: Some(metadata.clone()),
            metadata,
        })
    }
    fn read_source(&self) -> &File {
        #[cfg(windows)]
        {
            &self.reader_file
        }
        #[cfg(not(windows))]
        {
            &self.file
        }
    }
    pub(crate) fn diagnostic(&self, limit: usize) -> Result<String, String> {
        let mut bytes = Vec::new();
        self.reader()?
            .take(limit as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!(
                    "cannot read raw capture {}: {error}; source not consumed",
                    self.path.display()
                )
            })?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
    fn matches(&self, path: &Path) -> io::Result<bool> {
        let Some(file) = open_regular(path)? else {
            return Ok(false);
        };
        let now = file.metadata()?;
        let stopped = self
            .stopped
            .as_ref()
            .ok_or_else(|| io::Error::other("raw capture was not frozen after stop"))?;
        #[cfg(unix)]
        let same = {
            use std::os::unix::fs::MetadataExt;
            self.metadata.dev() == now.dev() && self.metadata.ino() == now.ino()
        };
        // Without a native file identity, retaining scratch is safer than
        // deleting a same-sized replacement. Publication itself remains usable.
        #[cfg(not(unix))]
        let same = false;
        Ok(same && stopped.len() == now.len() && stopped.modified()? == now.modified()?)
    }
    pub(crate) fn consume(&self) -> Result<(), String> {
        let operation = || -> io::Result<()> {
            if !self.matches(&self.path)? {
                return Err(io::Error::other(
                    "raw path changed or exact cleanup ownership is unavailable; no path was removed by cleanup",
                ));
            }
            let parent = self
                .path
                .parent()
                .ok_or_else(|| io::Error::other("raw capture has no parent"))?;
            #[cfg(test)]
            let _active = super::tests::Active::new(&self.path);
            let (stage, file) = Stage::create(parent)?;
            drop(file);
            // An atomic move, then a second identity check inside the private
            // directory, closes check/unlink's foreign-replacement gap.
            stage.settled.set(true);
            if let Err(error) = checkpoint(FaultPoint::CaptureMove) {
                let cleanup = stage.clean();
                return Err(io::Error::other(format!(
                    "{error}; original raw path retained{}",
                    cleanup.err().map_or(String::new(), |error| format!(
                        "; cleanup failed: {error}; inspect {}",
                        stage.directory.display()
                    ))
                )));
            }
            if let Err(error) = fs::rename(&self.path, &stage.path) {
                let cleanup = stage.clean();
                return Err(io::Error::other(format!(
                    "{error}; original raw path retained{}",
                    cleanup.err().map_or(String::new(), |error| format!(
                        "; cleanup failed: {error}; inspect {}",
                        stage.directory.display()
                    ))
                )));
            }
            if !self.matches(&stage.path).unwrap_or(false) {
                // Never overwrite a new arrival at the old pathname. Keep the
                // moved entry in private custody if no-replace restore fails.
                let restore = fs::symlink_metadata(&stage.path).and_then(|metadata| {
                    if !metadata.is_file() {
                        return Err(io::Error::other(
                            "moved entry is nonregular; automatic restore refused",
                        ));
                    }
                    fs::hard_link(&stage.path, &self.path)
                });
                match restore {
                    Ok(()) => {
                        return Err(io::Error::other(format!(
                            "raw identity changed during consumption; foreign entry restored and retained at {} and {}",
                            self.path.display(),
                            stage.path.display()
                        )));
                    }
                    Err(error) => {
                        return Err(io::Error::other(format!(
                            "raw identity changed during consumption; moved entry retained at {}; restore refused: {error}",
                            stage.path.display()
                        )));
                    }
                }
            }
            stage.clean().map_err(|error| {
                io::Error::other(format!(
                    "owned raw cleanup incomplete: {error}; inspect retained staging {}",
                    stage.directory.display()
                ))
            })
        };
        operation().map_err(|error| format!("raw capture {}: {error}", self.path.display()))
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowsFileId {
    volume: u64,
    id: [u8; 16],
}
#[cfg(any(windows, test))]
fn verify_reopened_identity(
    original: Option<WindowsFileId>,
    reopened: Option<WindowsFileId>,
) -> io::Result<()> {
    match (original, reopened) {
        (Some(a), Some(b)) if a != b => Err(io::Error::other(
            "reopened admitted capture identity differs",
        )),
        // ReOpenFile itself names the admitted object, not its current path.
        // If a filesystem cannot expose IDs, do not invent a positive ID proof
        // or disable otherwise valid handle-based read isolation.
        _ => Ok(()),
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn independent_windows_reader(original: &File) -> io::Result<File> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    #[repr(C)]
    struct Information {
        volume: u64,
        id: [u8; 16],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn ReOpenFile(
            original: *mut std::ffi::c_void,
            access: u32,
            share: u32,
            flags: u32,
        ) -> *mut std::ffi::c_void;
        fn SetHandleInformation(handle: *mut std::ffi::c_void, mask: u32, flags: u32) -> i32;
        fn GetFileInformationByHandleEx(
            handle: *mut std::ffi::c_void,
            class: i32,
            information: *mut std::ffi::c_void,
            length: u32,
        ) -> i32;
    }
    fn identity(file: &File) -> Option<WindowsFileId> {
        let mut information = std::mem::MaybeUninit::<Information>::uninit();
        // SAFETY: live borrowed File handle; FileIdInfo=18 fills the ABI struct
        // on success. Retain the entire 128-bit ID (including ReFS IDs).
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                18,
                information.as_mut_ptr().cast(),
                std::mem::size_of::<Information>() as u32,
            )
        } == 0
        {
            return None;
        }
        let information = unsafe { information.assume_init() };
        Some(WindowsFileId {
            volume: information.volume,
            id: information.id,
        })
    }
    // A new read-only description of the exact admitted object: not try_clone
    // (which shares the producer cursor), and never a pathname refresh.
    // Match std's READ|WRITE|DELETE sharing; OPEN_REPARSE_POINT avoids traversal.
    let handle = unsafe { ReOpenFile(original.as_raw_handle(), 0x8000_0000, 0x7, 0x0020_0000) };
    if handle.is_null() || handle as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful ReOpenFile transfers a distinct owned handle.
    let reader = unsafe { File::from_raw_handle(handle) };
    // HANDLE_FLAG_INHERIT is explicitly cleared before producer admission.
    if unsafe { SetHandleInformation(reader.as_raw_handle(), 1, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if !reader.metadata()?.is_file() {
        return Err(io::Error::other("reopened capture is not regular"));
    }
    verify_reopened_identity(identity(original), identity(&reader))?;
    Ok(reader)
}

/// Unix read_at leaves stdio untouched; Windows seek_read uses its separately
/// reopened read-only description, never the producer's shared cursor.
/// The stopped length bounds EOF even if a descendant keeps an output open.
pub(crate) struct CaptureReader<'a> {
    file: &'a File,
    offset: u64,
    length: u64,
}
impl Read for CaptureReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes.len().min(
            self.length
                .saturating_sub(self.offset)
                .min(usize::MAX as u64) as usize,
        );
        if count == 0 {
            return Ok(0);
        }
        #[cfg(unix)]
        let read = {
            use std::os::unix::fs::FileExt;
            self.file.read_at(&mut bytes[..count], self.offset)?
        };
        #[cfg(windows)]
        let read = {
            use std::os::windows::fs::FileExt;
            self.file.seek_read(&mut bytes[..count], self.offset)?
        };
        #[cfg(not(any(unix, windows)))]
        return Err(io::Error::other(
            "positional capture reads are unsupported on this platform",
        ));
        #[cfg(any(unix, windows))]
        {
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "admitted capture shortened after stop",
                ));
            }
            self.offset += read as u64;
            Ok(read)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopened_handle_identity_checks_all_128_bits_when_available() {
        let first = WindowsFileId {
            volume: 7,
            id: [0; 16],
        };
        assert!(verify_reopened_identity(Some(first), Some(first)).is_ok());
        let mut high = first;
        high.id[15] = 1;
        assert!(verify_reopened_identity(Some(first), Some(high)).is_err());
        assert!(
            verify_reopened_identity(Some(first), Some(WindowsFileId { volume: 8, ..first }))
                .is_err()
        );
        // Native ReOpenFile object authority does not become a fabricated ID
        // proof when the filesystem cannot answer either optional query.
        for pair in [(None, None), (Some(first), None), (None, Some(first))] {
            assert!(verify_reopened_identity(pair.0, pair.1).is_ok());
        }
    }

    #[test]
    fn admitted_read_object_survives_name_replacement_before_first_parse() {
        let root = std::env::temp_dir().join(format!(
            "asb-capture-read-object-{}",
            crate::tools::runtime::unique_id()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("capture");
        let mut capture = Capture::create(&path).unwrap();
        let mut writer = capture.writer().unwrap();
        writer.write_all(b"A=17\n").unwrap();
        fs::rename(&path, root.join("saved")).unwrap();
        fs::write(&path, b"A=99\n").unwrap();
        capture.freeze().unwrap();
        assert_eq!(capture.startup_diagnostic(3).unwrap(), "A=1");
        assert_eq!(capture.diagnostic(32).unwrap(), "A=17\n");
        let mut parsed = String::new();
        capture
            .reader()
            .unwrap()
            .read_to_string(&mut parsed)
            .unwrap();
        assert_eq!(parsed, "A=17\n");
        assert_eq!(fs::read(&path).unwrap(), b"A=99\n");
        drop(writer);
        drop(capture);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn reopened_windows_description_is_read_only() {
        let root = std::env::temp_dir().join(format!(
            "asb-capture-read-only-{}",
            crate::tools::runtime::unique_id()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("capture");
        let capture = Capture::create(&path).unwrap();
        assert!((&capture.reader_file).write_all(b"must not write").is_err());
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        drop(capture);
        fs::remove_dir_all(root).unwrap();
    }
}
