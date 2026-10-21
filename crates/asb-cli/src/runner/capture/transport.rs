//! Deadline-aware owned duplicates for the guest's actual controller pipes.
//! This is not a wrapper claiming cancellation of an arbitrary borrowed Write.
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::time::{Duration, Instant};

pub(crate) struct GuestTransport {
    file: File,
    flags: i32,
    deadline: Instant,
}
impl GuestTransport {
    pub(crate) fn new(source: &impl AsRawFd, deadline: Instant) -> io::Result<Self> {
        let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: fstat initializes the supplied stat on success; it changes no
        // descriptor flags. Never change a terminal or regular-file description.
        if unsafe { libc::fstat(source.as_raw_fd(), metadata.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let kind = unsafe { metadata.assume_init() }.st_mode & libc::S_IFMT;
        if !matches!(kind, libc::S_IFIFO | libc::S_IFSOCK) {
            return Err(io::Error::other(
                "guest output requires owned controller pipes or sockets",
            ));
        }
        // SAFETY: duplicate the supplied live descriptor into owned CLOEXEC
        // custody. Restore its shared file-description flags before closing.
        let fd = unsafe { libc::fcntl(source.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            file,
            flags,
            deadline,
        })
    }
    fn send(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        let deadline = self.deadline.min(Instant::now() + Duration::from_secs(1));
        while !bytes.is_empty() {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "guest output transport deadline exceeded",
                ));
            }
            match self.file.write(bytes) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(size) => bytes = &bytes[size..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
}
impl Write for GuestTransport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.send(bytes)?;
        Ok(bytes.len())
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.send(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Drop for GuestTransport {
    fn drop(&mut self) {
        // SAFETY: restore only the still-owned descriptor's original flags.
        unsafe {
            libc::fcntl(self.file.as_raw_fd(), libc::F_SETFL, self.flags);
        }
    }
}
