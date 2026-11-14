//! Which sockets a process holds (R-010, NCMD-25): the system's TCP and UDP tables, IPv4 and IPv6,
//! filtered by owning process. Read-only and unprivileged, so no ETW and no elevation. The tables are a
//! snapshot: a socket opened and closed between two reads is not seen (the caller records its interval).
use std::ffi::c_void;
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};

const AF_INET: u32 = 2;
const AF_INET6: u32 = 23;
const NO_ERROR: u32 = 0;
const INSUFFICIENT_BUFFER: u32 = 122;

/// One socket: its protocol and local port. Addresses are left out; nothing here is logged but a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Socket {
    pub proto: &'static str,
    pub port: u16,
}

/// Where the local port and the owning pid sit in one table row (byte offsets), and the row length.
struct Layout {
    proto: &'static str,
    row: usize,
    port: usize,
    pid: usize,
}

// MIB_TCPROW_OWNER_PID, MIB_TCP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, MIB_UDP6ROW_OWNER_PID.
const TCP4: Layout = Layout {
    proto: "tcp4",
    row: 24,
    port: 8,
    pid: 20,
};
const TCP6: Layout = Layout {
    proto: "tcp6",
    row: 56,
    port: 20,
    pid: 52,
};
const UDP4: Layout = Layout {
    proto: "udp4",
    row: 12,
    port: 4,
    pid: 8,
};
const UDP6: Layout = Layout {
    proto: "udp6",
    row: 28,
    port: 20,
    pid: 24,
};

fn word(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Every `(owner pid, socket)` in one table (`count` then rows). Ports are stored in network byte
/// order in the low 16 bits of a DWORD.
fn parse(table: &[u8], l: &Layout) -> Vec<(u32, Socket)> {
    let count = word(table, 0).unwrap_or(0) as usize;
    (0..count)
        .filter_map(|i| {
            let at = 4 + i * l.row;
            let (owner, port) = (word(table, at + l.pid)?, word(table, at + l.port)?);
            Some((
                owner,
                Socket {
                    proto: l.proto,
                    port: u16::from_be((port & 0xFFFF) as u16),
                },
            ))
        })
        .collect()
}

/// Calls `get(buffer, size)` until it fits (the table can grow between the size query and the read).
fn fetch(get: impl Fn(Option<*mut c_void>, *mut u32) -> u32) -> std::io::Result<Vec<u8>> {
    let mut size = 0u32;
    for _ in 0..8 {
        // A u32 buffer keeps the 4-byte alignment the table rows have.
        let mut buf = vec![0u32; (size as usize).div_ceil(4).max(1)];
        let mut given = size;
        let code = get(
            (size > 0).then(|| buf.as_mut_ptr().cast::<c_void>()),
            &mut given,
        );
        match code {
            NO_ERROR if size > 0 => {
                let bytes: Vec<u8> = buf.iter().flat_map(|w| w.to_le_bytes()).collect();
                return Ok(bytes);
            }
            // The first call only asks for the size; `given` is what the table needs now.
            NO_ERROR | INSUFFICIENT_BUFFER => size = given.saturating_add(4096),
            other => return Err(std::io::Error::from_raw_os_error(other as i32)),
        }
    }
    Err(std::io::Error::other("the socket table kept growing"))
}

/// Every TCP and UDP socket (IPv4 and IPv6, any state) on the machine right now, with its owner.
pub fn sockets() -> std::io::Result<Vec<(u32, Socket)>> {
    let mut found = Vec::new();
    for (family, layout) in [(AF_INET, TCP4), (AF_INET6, TCP6)] {
        // SAFETY: `fetch` passes a live buffer of the size it announces, or none for the size query;
        // `given` is a live u32. The table is only read as bytes afterwards.
        let table = fetch(|buf, given| unsafe {
            GetExtendedTcpTable(buf, given, false, family, TCP_TABLE_OWNER_PID_ALL, 0)
        })?;
        found.extend(parse(&table, &layout));
    }
    for (family, layout) in [(AF_INET, UDP4), (AF_INET6, UDP6)] {
        // SAFETY: as above, for the UDP table.
        let table = fetch(|buf, given| unsafe {
            GetExtendedUdpTable(buf, given, false, family, UDP_TABLE_OWNER_PID, 0)
        })?;
        found.extend(parse(&table, &layout));
    }
    Ok(found)
}

/// The sockets owned by `pid` right now.
pub fn sockets_of(pid: u32) -> std::io::Result<Vec<Socket>> {
    Ok(sockets()?
        .into_iter()
        .filter(|(owner, _)| *owner == pid)
        .map(|(_, s)| s)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut t = (rows.len() as u32).to_le_bytes().to_vec();
        for r in rows {
            t.extend(r);
        }
        t
    }

    fn udp4(port: u16, pid: u32) -> Vec<u8> {
        let mut r = vec![0u8; 4];
        r.extend(u32::from(port.to_be()).to_le_bytes());
        r.extend(pid.to_le_bytes());
        r
    }

    // @R-010
    #[test]
    fn r010_a_table_row_is_read_by_owner_and_port() {
        let t = table(&[udp4(5353, 7), udp4(80, 8), udp4(443, 7)]);
        let ports: Vec<(u32, u16)> = parse(&t, &UDP4)
            .iter()
            .map(|(pid, s)| (*pid, s.port))
            .collect();
        assert_eq!(ports, [(7, 5353), (8, 80), (7, 443)]);
        // A count that claims more rows than the buffer holds ends at the buffer, it does not panic.
        let mut cut = t.clone();
        cut.truncate(10);
        assert_eq!(parse(&cut, &UDP4).len(), 0);
        assert!(parse(&[], &UDP4).is_empty());
    }

    // @R-010
    #[test]
    fn r010_the_system_tables_show_this_process_own_sockets() {
        let me = std::process::id();
        let before = sockets_of(me).unwrap_or_else(|e| panic!("tables: {e}"));
        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap_or_else(|e| panic!("{e}"));
        let (tp, up) = (
            tcp.local_addr().map_or(0, |a| a.port()),
            udp.local_addr().map_or(0, |a| a.port()),
        );
        let after = sockets_of(me).unwrap_or_else(|e| panic!("tables: {e}"));
        assert!(after.contains(&Socket {
            proto: "tcp4",
            port: tp
        }));
        assert!(after.contains(&Socket {
            proto: "udp4",
            port: up
        }));
        assert!(after.len() >= before.len() + 2);
    }

    /// A row of `len` bytes with the port and the owner written where the layout says.
    fn row(l: &Layout, port: u16, pid: u32) -> Vec<u8> {
        let mut r = vec![0xAAu8; l.row];
        r[l.port..l.port + 4].copy_from_slice(&u32::from(port.to_be()).to_le_bytes());
        r[l.pid..l.pid + 4].copy_from_slice(&pid.to_le_bytes());
        r
    }

    // @R-010
    #[test]
    fn r010_every_table_layout_reads_its_own_port_and_owner() {
        // The IPv6 and TCP rows have other offsets than the UDP4 row the live test happens to meet.
        for l in [&TCP4, &TCP6, &UDP4, &UDP6] {
            let t = table(&[row(l, 8080, 41), row(l, 53, 42)]);
            let got: Vec<(u32, u16)> = parse(&t, l).iter().map(|(p, s)| (*p, s.port)).collect();
            assert_eq!(got, [(41, 8080), (42, 53)], "{}", l.proto);
        }
    }
}
