use std::ffi::CString;
use std::mem;
use std::ptr;
use std::sync::atomic::{fence, Ordering};
use std::time::{Duration, Instant};

use libc::{c_void, close, mmap, munmap, shm_open, MAP_FAILED, MAP_SHARED, O_RDWR, PROT_READ, PROT_WRITE};

pub const BIRD_SHM_NAME: &str = "/bird_shm_export";

pub const BIRD_SHM_MAGIC: u32 = 0x42524453;
pub const BIRD_SHM_VERSION: u32 = 1;
pub const BIRD_SHM_SNAPSHOT_VERSION: u32 = 1;

pub const BIRD_SHM_CMD_SNAPSHOT: u32 = 1;

pub const BIRD_SHM_MAX_INTERFACES: usize = 512;
pub const BIRD_SHM_MAX_IFACE_ADDRS: usize = 2048;
pub const BIRD_SHM_MAX_PROTOCOLS: usize = 512;
pub const BIRD_SHM_MAX_BGP: usize = 512;
pub const BIRD_SHM_MAX_OSPF: usize = 64;
pub const BIRD_SHM_MAX_OSPF_LSAS: usize = 2048;
pub const BIRD_SHM_MAX_OSPF_NEIGHBORS: usize = 1024;
pub const BIRD_SHM_MAX_BFD_SESSIONS: usize = 512;
pub const BIRD_SHM_MAX_BABEL: usize = 64;
pub const BIRD_SHM_MAX_BABEL_IFACES: usize = 256;
pub const BIRD_SHM_MAX_BABEL_NEIGHBORS: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmIpAddr {
    pub af: u8,
    pub pad: [u8; 3],
    pub bytes: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmStatus {
    pub boot_time: u64,
    pub current_time: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmIface {
    pub name: [u8; 16],
    pub flags: u32,
    pub mtu: u32,
    pub index: u32,
    pub addr_start: u32,
    pub addr_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmIfaceAddr {
    pub iface_index: u32,
    pub prefix_len: u16,
    pub scope: u16,
    pub flags: u32,
    pub ip: BirdShmIpAddr,
    pub brd: BirdShmIpAddr,
    pub opposite: BirdShmIpAddr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmProto {
    pub name: [u8; 32],
    pub class: u32,
    pub state: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmBgpInfo {
    pub name: [u8; 32],
    pub local_as: u32,
    pub remote_as: u32,
    pub conn_state: u8,
    pub pad: [u8; 3],
    pub remote_ip: BirdShmIpAddr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmOspfInfo {
    pub name: [u8; 32],
    pub router_id: u32,
    pub version: u8,
    pub pad: [u8; 3],
    pub lsa_start: u32,
    pub lsa_count: u32,
    pub neigh_start: u32,
    pub neigh_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmOspfLsa {
    pub proto_index: u32,
    pub lsa_type: u32,
    pub domain: u32,
    pub id: u32,
    pub rt: u32,
    pub sn: i32,
    pub age: u16,
    pub length: u16,
    pub type_raw: u16,
    pub pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmOspfNeighbor {
    pub proto_index: u32,
    pub ifname: [u8; 16],
    pub rid: u32,
    pub state: u8,
    pub pad: [u8; 3],
    pub ip: BirdShmIpAddr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmBfdSession {
    pub addr: BirdShmIpAddr,
    pub ifname: [u8; 16],
    pub state: u8,
    pub rem_state: u8,
    pub pad: [u8; 2],
    pub local_disc: u32,
    pub remote_disc: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmBabelInfo {
    pub name: [u8; 32],
    pub router_id: u64,
    pub update_seqno: u32,
    pub triggered: u8,
    pub pad: [u8; 3],
    pub iface_start: u32,
    pub iface_count: u32,
    pub neigh_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmBabelIface {
    pub proto_index: u32,
    pub ifname: [u8; 16],
    pub up: u8,
    pub pad: [u8; 3],
    pub tx_length: u32,
    pub hello_seqno: u16,
    pub pad2: u16,
    pub neigh_start: u32,
    pub neigh_count: u32,
    pub addr: BirdShmIpAddr,
    pub next_hop_ip4: BirdShmIpAddr,
    pub next_hop_ip6: BirdShmIpAddr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmBabelNeighbor {
    pub iface_index: u32,
    pub rxcost: u16,
    pub txcost: u16,
    pub cost: u16,
    pub hello_cnt: u8,
    pub pad: u8,
    pub last_hello_int: u32,
    pub last_tstamp: u32,
    pub srtt: u64,
    pub hello_expiry: u64,
    pub ihu_expiry: u64,
    pub addr: BirdShmIpAddr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BirdShmMailbox {
    pub cmd: u32,
    pub reserved: u32,
    pub arg0: u64,
    pub arg1: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BirdShmSnapshot {
    pub version: u32,
    pub trunc_flags: u32,
    pub last_cmd: u64,
    pub status: BirdShmStatus,

    pub iface_count: u32,
    pub iface_addr_count: u32,
    pub proto_count: u32,
    pub bgp_count: u32,
    pub ospf_count: u32,
    pub ospf_lsa_count: u32,
    pub ospf_neigh_count: u32,
    pub bfd_count: u32,
    pub babel_count: u32,
    pub babel_iface_count: u32,
    pub babel_neigh_count: u32,

    pub ifaces: [BirdShmIface; BIRD_SHM_MAX_INTERFACES],
    pub iface_addrs: [BirdShmIfaceAddr; BIRD_SHM_MAX_IFACE_ADDRS],
    pub protos: [BirdShmProto; BIRD_SHM_MAX_PROTOCOLS],
    pub bgp: [BirdShmBgpInfo; BIRD_SHM_MAX_BGP],
    pub ospf: [BirdShmOspfInfo; BIRD_SHM_MAX_OSPF],
    pub ospf_lsas: [BirdShmOspfLsa; BIRD_SHM_MAX_OSPF_LSAS],
    pub ospf_neighs: [BirdShmOspfNeighbor; BIRD_SHM_MAX_OSPF_NEIGHBORS],
    pub bfd: [BirdShmBfdSession; BIRD_SHM_MAX_BFD_SESSIONS],
    pub babel: [BirdShmBabelInfo; BIRD_SHM_MAX_BABEL],
    pub babel_ifaces: [BirdShmBabelIface; BIRD_SHM_MAX_BABEL_IFACES],
    pub babel_neighs: [BirdShmBabelNeighbor; BIRD_SHM_MAX_BABEL_NEIGHBORS],
}

impl Default for BirdShmSnapshot {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

#[repr(C)]
pub struct BirdShmRegion {
    pub magic: u32,
    pub version: u32,
    pub cmd_flag: u32,
    pub reserved: u32,
    pub version_seq: u64,
    pub mailbox: BirdShmMailbox,
    pub snapshot: BirdShmSnapshot,
}

pub struct ShmHandle {
    region: *mut BirdShmRegion,
    len: usize,
}

unsafe impl Send for ShmHandle {}
unsafe impl Sync for ShmHandle {}

impl ShmHandle {
    pub fn open() -> Result<Self, String> {
        let name = CString::new(BIRD_SHM_NAME).map_err(|e| e.to_string())?;
        let fd = unsafe { shm_open(name.as_ptr(), O_RDWR, 0o666) };
        if fd < 0 {
            return Err(format!("shm_open failed: {}", std::io::Error::last_os_error()));
        }

        let len = mem::size_of::<BirdShmRegion>();

        let addr = unsafe {
            mmap(
                ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            )
        };
        unsafe { close(fd) };

        if addr == MAP_FAILED {
            return Err(format!("mmap failed: {}", std::io::Error::last_os_error()));
        }

        let region = addr as *mut BirdShmRegion;
        let magic = unsafe { ptr::read_volatile(&(*region).magic) };
        let version = unsafe { ptr::read_volatile(&(*region).version) };
        if magic != BIRD_SHM_MAGIC || version != BIRD_SHM_VERSION {
            unsafe { munmap(addr, len) };
            return Err("SHM header mismatch".to_string());
        }

        Ok(Self { region, len })
    }

    pub fn request_snapshot(&self) -> Result<BirdShmSnapshot, String> {
        unsafe {
            (*self.region).mailbox.cmd = BIRD_SHM_CMD_SNAPSHOT;
            (*self.region).mailbox.arg0 = 0;
            (*self.region).mailbox.arg1 = 0;
        }

        self.write_cmd_flag(1);
        let start_seq = self.read_version_seq();
        let deadline = Instant::now() + Duration::from_secs(2);

        loop {
            let cmd = self.read_cmd_flag();
            let seq = self.read_version_seq();
            if cmd == 0 && seq != start_seq && (seq & 1) == 0 {
                break;
            }

            if Instant::now() > deadline {
                return Err("timeout waiting for snapshot".to_string());
            }

            std::thread::yield_now();
        }

        let snapshot = self.read_snapshot_with_deadline(deadline)?;
        if snapshot.version != BIRD_SHM_SNAPSHOT_VERSION {
            return Err("snapshot version mismatch".to_string());
        }

        Ok(snapshot)
    }

    fn read_snapshot_with_deadline(&self, deadline: Instant) -> Result<BirdShmSnapshot, String> {
        loop {
            let seq1 = self.read_version_seq();
            if (seq1 & 1) != 0 {
                continue;
            }

            let snapshot = unsafe { ptr::read(&(*self.region).snapshot) };
            fence(Ordering::Acquire);
            let seq2 = self.read_version_seq();

            if seq1 == seq2 && (seq2 & 1) == 0 {
                return Ok(snapshot);
            }

            if Instant::now() > deadline {
                return Err("timeout reading snapshot".to_string());
            }
        }
    }

    fn read_cmd_flag(&self) -> u32 {
        unsafe { ptr::read_volatile(&(*self.region).cmd_flag) }
    }

    fn write_cmd_flag(&self, val: u32) {
        unsafe { ptr::write_volatile(&mut (*self.region).cmd_flag, val) };
    }

    fn read_version_seq(&self) -> u64 {
        unsafe { ptr::read_volatile(&(*self.region).version_seq) }
    }
}

impl Drop for ShmHandle {
    fn drop(&mut self) {
        if !self.region.is_null() {
            unsafe {
                munmap(self.region as *mut c_void, self.len);
            }
        }
    }
}

pub fn cstr_to_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).to_string()
}
