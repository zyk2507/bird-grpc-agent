mod shm;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};

use shm::{
    cstr_to_string, BirdShmSnapshot, BirdShmIpAddr, ShmHandle,
    BIRD_SHM_MAX_BABEL_IFACES, BIRD_SHM_MAX_BABEL_NEIGHBORS, BIRD_SHM_MAX_IFACE_ADDRS,
    BIRD_SHM_MAX_INTERFACES, BIRD_SHM_MAX_OSPF_LSAS, BIRD_SHM_MAX_OSPF_NEIGHBORS,
};

pub mod exporter {
    tonic::include_proto!("birdexporter");
}

use exporter::bird_exporter_server::{BirdExporter, BirdExporterServer};
use exporter::*;

#[derive(Clone)]
struct BirdExporterSvc {
    shm: Arc<Mutex<ShmHandle>>,
}

impl BirdExporterSvc {
    async fn snapshot(&self) -> Result<BirdShmSnapshot, Status> {
        let shm = self.shm.lock().await;
        shm.request_snapshot()
            .map_err(|e| Status::unavailable(e))
    }
}

fn ip_to_proto(ip: &BirdShmIpAddr) -> IpAddr {
    let bytes = match ip.af {
        4 => ip.bytes[0..4].to_vec(),
        6 => ip.bytes[0..16].to_vec(),
        _ => Vec::new(),
    };

    IpAddr {
        af: ip.af as u32,
        bytes,
    }
}

fn iface_addr_to_proto(addr: &shm::BirdShmIfaceAddr) -> InterfaceAddress {
    InterfaceAddress {
        iface_index: addr.iface_index,
        prefix_len: addr.prefix_len as u32,
        scope: addr.scope as u32,
        flags: addr.flags,
        ip: Some(ip_to_proto(&addr.ip)),
        brd: Some(ip_to_proto(&addr.brd)),
        opposite: Some(ip_to_proto(&addr.opposite)),
    }
}

fn clamp_range(start: usize, count: usize, max: usize) -> (usize, usize) {
    if start >= max {
        return (max, max);
    }
    let end = start.saturating_add(count).min(max);
    (start, end)
}

fn build_interfaces(snapshot: &BirdShmSnapshot) -> Vec<Interface> {
    let mut interfaces = Vec::new();
    let iface_count = snapshot.iface_count.min(BIRD_SHM_MAX_INTERFACES as u32) as usize;

    for idx in 0..iface_count {
        let iface = snapshot.ifaces[idx];
        let start = iface.addr_start as usize;
        let count = iface.addr_count as usize;
        let (start, end) = clamp_range(start, count, BIRD_SHM_MAX_IFACE_ADDRS);
        let mut addrs = Vec::new();
        for addr in &snapshot.iface_addrs[start..end] {
            addrs.push(iface_addr_to_proto(addr));
        }

        interfaces.push(Interface {
            name: cstr_to_string(&iface.name),
            flags: iface.flags,
            mtu: iface.mtu,
            index: iface.index,
            addrs,
        });
    }

    interfaces
}

fn build_protocols(snapshot: &BirdShmSnapshot) -> Vec<ProtocolInfo> {
    let mut protos = Vec::new();
    let count = snapshot.proto_count.min(shm::BIRD_SHM_MAX_PROTOCOLS as u32) as usize;
    for idx in 0..count {
        let p = snapshot.protos[idx];
        protos.push(ProtocolInfo {
            name: cstr_to_string(&p.name),
            class: p.class,
            state: p.state,
        });
    }
    protos
}

fn build_bgp(snapshot: &BirdShmSnapshot) -> Vec<BgpInfo> {
    let mut sessions = Vec::new();
    let count = snapshot.bgp_count.min(shm::BIRD_SHM_MAX_BGP as u32) as usize;
    for idx in 0..count {
        let b = snapshot.bgp[idx];
        sessions.push(BgpInfo {
            name: cstr_to_string(&b.name),
            local_as: b.local_as,
            remote_as: b.remote_as,
            conn_state: b.conn_state as u32,
            remote_ip: Some(ip_to_proto(&b.remote_ip)),
        });
    }
    sessions
}

fn build_ospf(snapshot: &BirdShmSnapshot) -> Vec<OspfInfo> {
    let mut instances = Vec::new();
    let count = snapshot.ospf_count.min(shm::BIRD_SHM_MAX_OSPF as u32) as usize;

    for idx in 0..count {
        let o = snapshot.ospf[idx];
        let lsa_start = o.lsa_start as usize;
        let (lsa_start, lsa_end) = clamp_range(lsa_start, o.lsa_count as usize, BIRD_SHM_MAX_OSPF_LSAS);
        let neigh_start = o.neigh_start as usize;
        let (neigh_start, neigh_end) = clamp_range(neigh_start, o.neigh_count as usize, BIRD_SHM_MAX_OSPF_NEIGHBORS);

        let mut lsas = Vec::new();
        for lsa in &snapshot.ospf_lsas[lsa_start..lsa_end] {
            lsas.push(OspfLsa {
                proto_index: lsa.proto_index,
                lsa_type: lsa.lsa_type,
                domain: lsa.domain,
                id: lsa.id,
                rt: lsa.rt,
                sn: lsa.sn,
                age: lsa.age as u32,
                length: lsa.length as u32,
                type_raw: lsa.type_raw as u32,
            });
        }

        let mut neighbors = Vec::new();
        for n in &snapshot.ospf_neighs[neigh_start..neigh_end] {
            neighbors.push(OspfNeighbor {
                proto_index: n.proto_index,
                ifname: cstr_to_string(&n.ifname),
                rid: n.rid,
                state: n.state as u32,
                ip: Some(ip_to_proto(&n.ip)),
            });
        }

        instances.push(OspfInfo {
            name: cstr_to_string(&o.name),
            router_id: o.router_id,
            version: o.version as u32,
            lsas,
            neighbors,
        });
    }

    instances
}

fn build_bfd(snapshot: &BirdShmSnapshot) -> Vec<BfdSession> {
    let mut sessions = Vec::new();
    let count = snapshot.bfd_count.min(shm::BIRD_SHM_MAX_BFD_SESSIONS as u32) as usize;
    for idx in 0..count {
        let s = snapshot.bfd[idx];
        sessions.push(BfdSession {
            addr: Some(ip_to_proto(&s.addr)),
            ifname: cstr_to_string(&s.ifname),
            state: s.state as u32,
            rem_state: s.rem_state as u32,
            local_disc: s.local_disc,
            remote_disc: s.remote_disc,
        });
    }
    sessions
}

fn build_babel(snapshot: &BirdShmSnapshot) -> Vec<BabelInfo> {
    let mut instances = Vec::new();
    let count = snapshot.babel_count.min(shm::BIRD_SHM_MAX_BABEL as u32) as usize;

    for idx in 0..count {
        let b = snapshot.babel[idx];
        let iface_start = b.iface_start as usize;
        let (iface_start, iface_end) = clamp_range(iface_start, b.iface_count as usize, BIRD_SHM_MAX_BABEL_IFACES);
        let mut interfaces = Vec::new();

        for (offset, iface) in snapshot.babel_ifaces[iface_start..iface_end].iter().enumerate() {
            let _iface_index = iface_start + offset;
            let neigh_start = iface.neigh_start as usize;
            let (neigh_start, neigh_end) = clamp_range(neigh_start, iface.neigh_count as usize, BIRD_SHM_MAX_BABEL_NEIGHBORS);
            let mut neighbors = Vec::new();

            for n in &snapshot.babel_neighs[neigh_start..neigh_end] {
                neighbors.push(BabelNeighbor {
                    iface_index: n.iface_index,
                    rxcost: n.rxcost as u32,
                    txcost: n.txcost as u32,
                    cost: n.cost as u32,
                    hello_cnt: n.hello_cnt as u32,
                    last_hello_int: n.last_hello_int,
                    last_tstamp: n.last_tstamp,
                    srtt: n.srtt,
                    hello_expiry: n.hello_expiry,
                    ihu_expiry: n.ihu_expiry,
                    addr: Some(ip_to_proto(&n.addr)),
                });
            }

            interfaces.push(BabelInterface {
                proto_index: iface.proto_index,
                ifname: cstr_to_string(&iface.ifname),
                up: iface.up != 0,
                tx_length: iface.tx_length,
                hello_seqno: iface.hello_seqno as u32,
                neighbors,
                addr: Some(ip_to_proto(&iface.addr)),
                next_hop_ip4: Some(ip_to_proto(&iface.next_hop_ip4)),
                next_hop_ip6: Some(ip_to_proto(&iface.next_hop_ip6)),
            });

            let _ = _iface_index; // explicit for clarity if we need it later
        }

        instances.push(BabelInfo {
            name: cstr_to_string(&b.name),
            router_id: b.router_id,
            update_seqno: b.update_seqno,
            triggered: b.triggered != 0,
            interfaces,
            neigh_count: b.neigh_count,
        });
    }

    instances
}

struct InstanceLock {
    _file: std::fs::File,
}

impl InstanceLock {
    fn acquire() -> Result<Self, String> {
        let lock_path = create_lock_dir()
            .map(|dir| dir.join("bird-grpc-agent.lock"))
            .map_err(|e| format!("failed to prepare lock dir: {}", e))?;

        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| format!("failed to open lock file {}: {}", lock_path.display(), e))?;

        let fd = file.as_raw_fd();
        let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            return Err("another bird-grpc-agent instance is already running".to_string());
        }

        let _ = file.set_len(0);
        let _ = file.write_all(std::process::id().to_string().as_bytes());

        Ok(Self { _file: file })
    }
}

fn create_lock_dir() -> Result<std::path::PathBuf, std::io::Error> {
    let candidates = ["/run/bird-grpc-agent", "/var/run/bird-grpc-agent", "/tmp/bird-grpc-agent"];
    for dir in candidates {
        if fs::create_dir_all(dir).is_ok() {
            return Ok(std::path::PathBuf::from(dir));
        }
    }

    fs::create_dir_all(candidates[0])?;
    Ok(std::path::PathBuf::from(candidates[0]))
}

#[tonic::async_trait]
impl BirdExporter for BirdExporterSvc {
    async fn get_status(&self, _req: Request<Empty>) -> Result<Response<StatusResponse>, Status> {
        let snapshot = self.snapshot().await?;
        let status = RouterStatus {
            boot_time: snapshot.status.boot_time,
            current_time: snapshot.status.current_time,
        };

        Ok(Response::new(StatusResponse {
            status: Some(status),
            trunc_flags: snapshot.trunc_flags,
            last_cmd: snapshot.last_cmd,
        }))
    }

    async fn list_interfaces(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<InterfacesResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(InterfacesResponse {
            interfaces: build_interfaces(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn list_protocols(
        &self,
        _req: Request<Empty>,
    ) -> Result<Response<ProtocolsResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(ProtocolsResponse {
            protocols: build_protocols(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn list_bgp(&self, _req: Request<Empty>) -> Result<Response<BgpResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(BgpResponse {
            sessions: build_bgp(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn list_ospf(&self, _req: Request<Empty>) -> Result<Response<OspfResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(OspfResponse {
            instances: build_ospf(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn list_bfd(&self, _req: Request<Empty>) -> Result<Response<BfdResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(BfdResponse {
            sessions: build_bfd(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn list_babel(&self, _req: Request<Empty>) -> Result<Response<BabelResponse>, Status> {
        let snapshot = self.snapshot().await?;
        Ok(Response::new(BabelResponse {
            instances: build_babel(&snapshot),
            trunc_flags: snapshot.trunc_flags,
        }))
    }

    async fn get_snapshot(&self, _req: Request<Empty>) -> Result<Response<Snapshot>, Status> {
        let snapshot = self.snapshot().await?;
        let status = RouterStatus {
            boot_time: snapshot.status.boot_time,
            current_time: snapshot.status.current_time,
        };

        Ok(Response::new(Snapshot {
            status: Some(status),
            interfaces: build_interfaces(&snapshot),
            protocols: build_protocols(&snapshot),
            bgp: build_bgp(&snapshot),
            ospf: build_ospf(&snapshot),
            bfd: build_bfd(&snapshot),
            babel: build_babel(&snapshot),
            trunc_flags: snapshot.trunc_flags,
            last_cmd: snapshot.last_cmd,
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env::var("BIRD_GRPC_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    let addr = addr.parse()?;

    let _instance_lock = InstanceLock::acquire()?;

    let shm = ShmHandle::open()?;
    let svc = BirdExporterSvc {
        shm: Arc::new(Mutex::new(shm)),
    };

    Server::builder()
        .add_service(BirdExporterServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
