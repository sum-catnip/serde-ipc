use std::io;
use std::io::Read;
use std::env;
use std::path::PathBuf;
use std::mem;
use std::result;
use std::cmp;
use std::thread;
use std::time;

use shared_memory::{
    SharedMem,
    SharedMemCast,
    SharedMemError,
    LockType,
    WriteLockable,
    EventSet,
    EventWait,
    EventState,
    Timeout,
    SharedMemConf,
    EventType
};

use log::debug;
use serde::Serialize;
use serde::de::DeserializeOwned;
use bincode;
use bincode::Error as BinError;
use bincode::BincodeRead;
use snafu::{ Snafu, ResultExt, ensure };

#[cfg(test)]
mod tests {
    use super::*;
    use simple_logger;

    #[test]
    fn server_write_smoll() {
        let _ = simple_logger::init();
        let mut srv = IpcNode::new(1337).unwrap();
        assert!(srv.send(&(12, "topkek")).is_ok());
    }

    #[test]
    fn server_write_big() {
        let _ = simple_logger::init();
        let mut srv = IpcNode::new(123).unwrap();
        assert!(srv.send(&vec![7; 1000]).is_ok());
    }
    /*
    #[test]
    fn server_write_perfectfitx4() {
        let _ = simple_logger::init();
        let mut srv = IpcServer::new(69).unwrap();
        assert!(srv.send(&vec![7u8; BUFSIZE * 4]).is_ok());
    }*/
}

const BUFSIZE: usize = 1024;
const DATA_EVT: usize = 0;
const SPACE_EVT: usize = 1;

#[derive(SharedMemCast)]
struct ShmemMessage {
    buf: [u8; BUFSIZE],
    head: usize,
    tail: usize,
    full: bool
}

impl ShmemMessage {
    pub fn init(&mut self) {
        self.buf = [0; BUFSIZE];
        self.head = 0;
        self.tail = 0;
        self.full = false;
    }

    pub fn full(&self) -> bool { self.full }
    pub fn empty(&self) -> bool { !self.full && (self.head == self.tail) }
    fn advance(&mut self) {
        if self.full() { self.tail = (self.tail +1) % self.buf.len(); }
        self.head = (self.head +1) % self.buf.len();
        self.full = self.head == self.tail;
    }

    fn retreat(&mut self) {
        self.full = false;
        self.tail = (self.tail +1) % self.buf.len();
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        while !self.full() && written < data.len() {
            self.buf[self.head] = data[written];
            self.advance();
            written += 1;
        }

        written
    }

    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut written = 0;
        while !self.empty() && written < buf.len() {
            buf[written] = self.buf[self.tail];
            self.retreat();
            written += 1;
        }

        written
    }
}

#[derive(Debug, Snafu)]
pub enum IpcError {
    #[snafu(display("error creating shared memory with id {}: {}", id, source))]
    ShmemCreateError { source: SharedMemError, id: u64 },
    #[snafu(display("error opening shared memory with id {}: {}", id, source))]
    ShmemOpenError { source: SharedMemError, id: u64 },
    #[snafu(display("ipc file for id {} already exists", id))]
    FileExistsError { id: u64 },
    #[snafu(display("error writing to shared memory id: {}: {}", id, source))]
    ShmemWriteError { source: SharedMemError, id: u64 },
    #[snafu(display("{} ran out of shared memory. please call `recv` before sending more data", id))]
    OutOfMemortError { id: u64 },
    #[snafu(display("serialization error: {}", source))]
    SerializationError { source: BinError },
    #[snafu(display("deserialization error: {}", source))]
    DeserializationError { source: BinError }
}

pub type Result<T, E = IpcError> = result::Result<T, E>;

pub struct IpcNode {
    shmem: SharedMem,
    cache: ShmemCache,
    id: u64
}

impl IpcNode {
    pub fn new(id: u64) -> Result<Self> {
        let mut path = PathBuf::new();
        path.push(env::temp_dir());
        path.push(format!("serde-ipc-{}", id));

        let mut shmem = SharedMemConf::default()
            .set_link_path(path)
            .set_size(mem::size_of::<ShmemMessage>())
            .add_event(EventType::AutoBusy) // space available event
            .unwrap()
            .add_event(EventType::AutoBusy) // data available event
            .unwrap()
            .add_lock(LockType::Mutex, 0, mem::size_of::<ShmemMessage>())
            .unwrap()
            .create()
            .context(ShmemCreateError { id })?;

        ensure!(shmem.is_owner(), FileExistsError { id });
        {
            let mut msg = shmem.wlock::<ShmemMessage>(0).context(ShmemWriteError { id })?;
            msg.init();
        }
        Ok(IpcNode { shmem, id, cache: ShmemCache::new()})
    }

    pub fn open(id: u64) -> Result<Self> {
        let mut path = PathBuf::new();
        path.push(env::temp_dir());
        path.push(format!("serde-ipc-{}", id));

        let shmem = SharedMem::open_linked(path)
            .context(ShmemOpenError { id })?;

        Ok(IpcNode { shmem, id, cache: ShmemCache::new()})
    }

    pub fn send<T: ?Sized>(&mut self, val: &T) -> Result<()>
        where T: Serialize {
        let serialized = bincode::serialize(val)
            .context(SerializationError)?;

        let mut data_pos = 0;
        let mut wait = false;
        // we do this in a loop to check for new space
        while data_pos < serialized.len() {
            /*if wait {
                debug!("full buffer, waiting for space");
                self.shmem.wait(SPACE_EVT, Timeout::Infinite).unwrap();
                debug!("waiting for new space done");
            }*/
            {
                let mut msg = self.shmem.wlock::<ShmemMessage>(0)
                    .context(ShmemWriteError { id: self.id })?;
                
                data_pos += msg.write(&serialized[data_pos..]);
                debug!("write data pos: {}", data_pos);
                wait = msg.full();
            }
            //self.shmem.set(DATA_EVT, EventState::Signaled).unwrap();
        }

        Ok(())
    }

    pub fn recv<T>(&mut self) -> Result<T>
        where T: DeserializeOwned {
        bincode::deserialize_from(BufferedShmemReader::new(&mut self.shmem, self.id, &mut self.cache))
            .context(DeserializationError {})
    }
}

struct ShmemCache {
    buf: [u8; BUFSIZE],
    head: usize,
    tail: usize
}

impl ShmemCache {
    pub fn new() -> Self {
        ShmemCache {
            buf: [0; BUFSIZE],
            head: 0,
            tail: 0
        }
    }
}

struct BufferedShmemReader<'a> {
    shmem: &'a mut SharedMem,
    cache: &'a mut ShmemCache,
    id: u64
}

impl<'a> BufferedShmemReader<'a> {
    pub fn new(shmem: &'a mut SharedMem, id: u64, cache: &'a mut ShmemCache) -> Self {
        BufferedShmemReader { shmem, id, cache }
    }
}

impl<'a> Read for BufferedShmemReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut data_pos = 0;
        let id = self.id;
        let mut wait = false;
        while data_pos < buf.len() {
            debug!("cache head: {} tail: {}", self.cache.head, self.cache.tail);
            if self.cache.head != self.cache.tail {
                //debug!("found cached data");
                let remain = cmp::min(buf.len(), self.cache.tail - self.cache.head);
                debug!("reading {} cached bytes from {}, to: {}", remain, self.cache.head, self.cache.tail);
                buf[data_pos..data_pos + remain]
                    .copy_from_slice(&self.cache.buf[self.cache.head..self.cache.head + remain]);

                self.cache.head += remain;
                data_pos += remain;
            }
            /*if wait {
                debug!("empty buffer, waiting for new bytes");
            }*/
            else {
                let mut msg = self.shmem.wlock::<ShmemMessage>(0)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other,
                                                format!("error getting shmem @{} write lock: {}",
                                                        id, e)))?;
                
                let len = msg.read(&mut self.cache.buf);
                debug!("read {} bytes into cache. pos: {}, req: {}", len, data_pos, buf.len());
                self.cache.head = 0;
                self.cache.tail = len;
                wait = msg.empty();
            }
            // self.shmem.set(SPACE_EVT, EventState::Signaled).unwrap();
        }

        Ok(data_pos)
    }
}
