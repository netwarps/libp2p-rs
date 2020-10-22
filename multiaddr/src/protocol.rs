use crate::onion_addr::Onion3Addr;
use crate::{Error, Result};
use arrayref::array_ref;
use byteorder::{BigEndian, ByteOrder, ReadBytesExt, WriteBytesExt};
use data_encoding::BASE32;
use multihash::Multihash;
pub use multihash::{Code, Sha2_256};
use std::{
    borrow::Cow,
    convert::From,
    fmt,
    io::{Cursor, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::{self, FromStr},
};
use unsigned_varint::{decode, encode};

pub const DCCP: u32 = 33;
pub const DNS: u32 = 53;
pub const DNS4: u32 = 54;
pub const DNS6: u32 = 55;
pub const DNSADDR: u32 = 56;
pub const HTTP: u32 = 480;
pub const HTTPS: u32 = 443;
pub const IP4: u32 = 4;
pub const IP6: u32 = 41;
pub const P2P_WEBRTC_DIRECT: u32 = 276;
pub const P2P_WEBRTC_STAR: u32 = 275;
pub const P2P_WEBSOCKET_STAR: u32 = 479;
pub const MEMORY: u32 = 777;
pub const ONION: u32 = 444;
pub const ONION3: u32 = 445;
pub const P2P: u32 = 421;
pub const P2P_CIRCUIT: u32 = 290;
pub const QUIC: u32 = 460;
pub const SCTP: u32 = 132;
pub const TCP: u32 = 6;
pub const UDP: u32 = 273;
pub const UDT: u32 = 301;
pub const UNIX: u32 = 400;
pub const UTP: u32 = 302;
pub const WS: u32 = 477;
pub const WS_WITH_PATH: u32 = 4770; // Note: not standard
pub const WSS: u32 = 478;
pub const WSS_WITH_PATH: u32 = 4780; // Note: not standard

const PATH_SEGMENT_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b'%')
    .add(b'/')
    .add(b'`')
    .add(b'?')
    .add(b'{')
    .add(b'}')
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>');

/// `Protocol` describes all possible multiaddress protocols.
///
/// For `Unix`, `Ws` and `Wss` we use `&str` instead of `Path` to allow
/// cross-platform usage of `Protocol` since encoding `Paths` to bytes is
/// platform-specific. This means that the actual validation of paths needs to
/// happen separately.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Protocol<'a> {
    Dccp(u16),
    Dns(Cow<'a, str>),
    Dns4(Cow<'a, str>),
    Dns6(Cow<'a, str>),
    Dnsaddr(Cow<'a, str>),
    Http,
    Https,
    Ip4(Ipv4Addr),
    Ip6(Ipv6Addr),
    P2pWebRtcDirect,
    P2pWebRtcStar,
    P2pWebSocketStar,
    /// Contains the "port" to contact. Similar to TCP or UDP, 0 means "assign me a port".
    Memory(u64),
    Onion(Cow<'a, [u8; 10]>, u16),
    Onion3(Onion3Addr<'a>),
    P2p(Multihash),
    P2pCircuit,
    Quic,
    Sctp(u16),
    Tcp(u16),
    Udp(u16),
    Udt,
    Unix(Cow<'a, str>),
    Utp,
    Ws(Cow<'a, str>),
    Wss(Cow<'a, str>),
}

impl<'a> Protocol<'a> {
    /// Parse a protocol value from the given iterator of string slices.
    ///
    /// The parsing only consumes the minimum amount of string slices necessary to
    /// produce a well-formed protocol. The same iterator can thus be used to parse
    /// a sequence of protocols in succession. It is up to client code to check
    /// that iteration has finished whenever appropriate.
    pub fn from_str_parts<I>(mut iter: I) -> Result<Self>
    where
        I: Iterator<Item = &'a str>,
    {
        match iter.next().ok_or(Error::InvalidProtocolString)? {
            "ip4" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Ip4(Ipv4Addr::from_str(s)?))
            }
            "tcp" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Tcp(s.parse()?))
            }
            "udp" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Udp(s.parse()?))
            }
            "dccp" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Dccp(s.parse()?))
            }
            "ip6" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Ip6(Ipv6Addr::from_str(s)?))
            }
            "dns" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Dns(Cow::Borrowed(s)))
            }
            "dns4" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Dns4(Cow::Borrowed(s)))
            }
            "dns6" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Dns6(Cow::Borrowed(s)))
            }
            "dnsaddr" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Dnsaddr(Cow::Borrowed(s)))
            }
            "sctp" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Sctp(s.parse()?))
            }
            "udt" => Ok(Protocol::Udt),
            "utp" => Ok(Protocol::Utp),
            "unix" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Unix(Cow::Borrowed(s)))
            }
            "p2p" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                let decoded = bs58::decode(s).into_vec()?;
                Ok(Protocol::P2p(Multihash::from_bytes(decoded)?))
            }
            "http" => Ok(Protocol::Http),
            "https" => Ok(Protocol::Https),
            "onion" => iter
                .next()
                .ok_or(Error::InvalidProtocolString)
                .and_then(|s| read_onion(&s.to_uppercase()))
                .map(|(a, p)| Protocol::Onion(Cow::Owned(a), p)),
            "onion3" => iter
                .next()
                .ok_or(Error::InvalidProtocolString)
                .and_then(|s| read_onion3(&s.to_uppercase()))
                .map(|(a, p)| Protocol::Onion3((a, p).into())),
            "quic" => Ok(Protocol::Quic),
            "ws" => Ok(Protocol::Ws(Cow::Borrowed("/"))),
            "wss" => Ok(Protocol::Wss(Cow::Borrowed("/"))),
            "x-parity-ws" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                let decoded = percent_encoding::percent_decode(s.as_bytes()).decode_utf8()?;
                Ok(Protocol::Ws(decoded))
            }
            "x-parity-wss" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                let decoded = percent_encoding::percent_decode(s.as_bytes()).decode_utf8()?;
                Ok(Protocol::Wss(decoded))
            }
            "p2p-websocket-star" => Ok(Protocol::P2pWebSocketStar),
            "p2p-webrtc-star" => Ok(Protocol::P2pWebRtcStar),
            "p2p-webrtc-direct" => Ok(Protocol::P2pWebRtcDirect),
            "p2p-circuit" => Ok(Protocol::P2pCircuit),
            "memory" => {
                let s = iter.next().ok_or(Error::InvalidProtocolString)?;
                Ok(Protocol::Memory(s.parse()?))
            }
            unknown => Err(Error::UnknownProtocolString(unknown.to_string())),
        }
    }

    pub fn get_enum(id: u32) -> Result<Self> {
        match id {
            IP4 => Ok(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1))),
            IP6 => Ok(Protocol::Ip6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0xc00a, 0x2ff))),
            MEMORY => Ok(Protocol::Memory(0)),
            ONION => Ok(Protocol::Onion(Cow::Owned([0_u8; 10]), 0)),
            ONION3 => Ok(Protocol::Onion3(Onion3Addr::from(([0_u8; 35], 0)))),
            TCP => Ok(Protocol::Tcp(0)),
            UDP => Ok(Protocol::Udp(0)),
            SCTP => Ok(Protocol::Sctp(0)),
            UDT => Ok(Protocol::Udt),
            UTP => Ok(Protocol::Utp),
            UNIX => Ok(Protocol::Unix(Cow::Borrowed(""))),
            QUIC => Ok(Protocol::Quic),
            DCCP => Ok(Protocol::Dccp(0)),
            DNS => Ok(Protocol::Dns(Cow::Borrowed(""))),
            DNS4 => Ok(Protocol::Dns4(Cow::Borrowed(""))),
            DNS6 => Ok(Protocol::Dns6(Cow::Borrowed(""))),
            DNSADDR => Ok(Protocol::Dnsaddr(Cow::Borrowed(""))),
            WS => Ok(Protocol::Ws(Cow::Borrowed("/"))),
            WSS => Ok(Protocol::Wss(Cow::Borrowed("/"))),
            HTTP => Ok(Protocol::Http),
            HTTPS => Ok(Protocol::Https),
            P2P => Ok(Protocol::P2p(multihash::wrap(Code::Sha2_256, &Sha2_256::digest(b"0").digest()))),
            P2P_CIRCUIT => Ok(Protocol::P2pCircuit),
            P2P_WEBRTC_DIRECT => Ok(Protocol::P2pWebRtcDirect),
            P2P_WEBRTC_STAR => Ok(Protocol::P2pWebRtcStar),
            P2P_WEBSOCKET_STAR => Ok(Protocol::P2pWebSocketStar),
            _ => Err(Error::UnknownProtocolId(id)),
        }
    }

    pub fn get_key(&self) -> Result<u32> {
        match self {
            Protocol::Ip4(_) => Ok(IP4),
            Protocol::Ip6(_) => Ok(IP6),
            Protocol::Memory(_) => Ok(MEMORY),
            Protocol::Onion(_, _) => Ok(ONION),
            Protocol::Onion3(_) => Ok(ONION3),
            Protocol::Tcp(_) => Ok(TCP),
            Protocol::Udp(_) => Ok(UDP),
            Protocol::Sctp(_) => Ok(SCTP),
            Protocol::Udt => Ok(UDT),
            Protocol::Utp => Ok(UTP),
            Protocol::Unix(_) => Ok(UNIX),
            Protocol::Quic => Ok(QUIC),
            Protocol::Dccp(_) => Ok(DCCP),
            Protocol::Dns(_) => Ok(DNS),
            Protocol::Dns4(_) => Ok(DNS4),
            Protocol::Dns6(_) => Ok(DNS6),
            Protocol::Dnsaddr(_) => Ok(DNSADDR),
            Protocol::Wss(_) => Ok(WSS),
            Protocol::Http => Ok(HTTP),
            Protocol::Https => Ok(HTTPS),
            Protocol::P2pCircuit => Ok(P2P_CIRCUIT),
            Protocol::P2pWebRtcDirect => Ok(P2P_WEBRTC_DIRECT),
            Protocol::P2pWebRtcStar => Ok(P2P_WEBRTC_STAR),
            Protocol::P2pWebSocketStar => Ok(P2P_WEBSOCKET_STAR),
            _ => Err(Error::InvalidProtocolString),
        }
    }
    /// Parse a single `Protocol` value from its byte slice representation,
    /// returning the protocol as well as the remaining byte slice.
    pub fn from_bytes(input: &'a [u8]) -> Result<(Self, &'a [u8])> {
        fn split_at(n: usize, input: &[u8]) -> Result<(&[u8], &[u8])> {
            if input.len() < n {
                return Err(Error::DataLessThanLen);
            }
            Ok(input.split_at(n))
        }
        let (id, input) = decode::u32(input)?;
        match id {
            DCCP => {
                let (data, rest) = split_at(2, input)?;
                let mut rdr = Cursor::new(data);
                let num = rdr.read_u16::<BigEndian>()?;
                Ok((Protocol::Dccp(num), rest))
            }
            DNS => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Dns(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            DNS4 => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Dns4(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            DNS6 => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Dns6(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            DNSADDR => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Dnsaddr(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            HTTP => Ok((Protocol::Http, input)),
            HTTPS => Ok((Protocol::Https, input)),
            IP4 => {
                let (data, rest) = split_at(4, input)?;
                Ok((Protocol::Ip4(Ipv4Addr::new(data[0], data[1], data[2], data[3])), rest))
            }
            IP6 => {
                let (data, rest) = split_at(16, input)?;
                let mut rdr = Cursor::new(data);
                let mut seg = [0_u16; 8];

                for x in seg.iter_mut() {
                    *x = rdr.read_u16::<BigEndian>()?;
                }

                let addr = Ipv6Addr::new(seg[0], seg[1], seg[2], seg[3], seg[4], seg[5], seg[6], seg[7]);

                Ok((Protocol::Ip6(addr), rest))
            }
            P2P_WEBRTC_DIRECT => Ok((Protocol::P2pWebRtcDirect, input)),
            P2P_WEBRTC_STAR => Ok((Protocol::P2pWebRtcStar, input)),
            P2P_WEBSOCKET_STAR => Ok((Protocol::P2pWebSocketStar, input)),
            MEMORY => {
                let (data, rest) = split_at(8, input)?;
                let mut rdr = Cursor::new(data);
                let num = rdr.read_u64::<BigEndian>()?;
                Ok((Protocol::Memory(num), rest))
            }
            ONION => {
                let (data, rest) = split_at(12, input)?;
                let port = BigEndian::read_u16(&data[10..]);
                Ok((Protocol::Onion(Cow::Borrowed(array_ref!(data, 0, 10)), port), rest))
            }
            ONION3 => {
                let (data, rest) = split_at(37, input)?;
                let port = BigEndian::read_u16(&data[35..]);
                Ok((Protocol::Onion3((array_ref!(data, 0, 35), port).into()), rest))
            }
            P2P => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::P2p(Multihash::from_bytes(data.to_owned())?), rest))
            }
            P2P_CIRCUIT => Ok((Protocol::P2pCircuit, input)),
            QUIC => Ok((Protocol::Quic, input)),
            SCTP => {
                let (data, rest) = split_at(2, input)?;
                let mut rdr = Cursor::new(data);
                let num = rdr.read_u16::<BigEndian>()?;
                Ok((Protocol::Sctp(num), rest))
            }
            TCP => {
                let (data, rest) = split_at(2, input)?;
                let mut rdr = Cursor::new(data);
                let num = rdr.read_u16::<BigEndian>()?;
                Ok((Protocol::Tcp(num), rest))
            }
            UDP => {
                let (data, rest) = split_at(2, input)?;
                let mut rdr = Cursor::new(data);
                let num = rdr.read_u16::<BigEndian>()?;
                Ok((Protocol::Udp(num), rest))
            }
            UDT => Ok((Protocol::Udt, input)),
            UNIX => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Unix(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            UTP => Ok((Protocol::Utp, input)),
            WS => Ok((Protocol::Ws(Cow::Borrowed("/")), input)),
            WS_WITH_PATH => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Ws(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            WSS => Ok((Protocol::Wss(Cow::Borrowed("/")), input)),
            WSS_WITH_PATH => {
                let (n, input) = decode::usize(input)?;
                let (data, rest) = split_at(n, input)?;
                Ok((Protocol::Wss(Cow::Borrowed(str::from_utf8(data)?)), rest))
            }
            _ => Err(Error::UnknownProtocolId(id)),
        }
    }

    /// Encode this protocol by writing its binary representation into
    /// the given `Write` impl.
    pub fn write_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = encode::u32_buffer();
        match self {
            Protocol::Ip4(addr) => {
                w.write_all(encode::u32(IP4, &mut buf))?;
                w.write_all(&addr.octets())?
            }
            Protocol::Ip6(addr) => {
                w.write_all(encode::u32(IP6, &mut buf))?;
                for &segment in &addr.segments() {
                    w.write_u16::<BigEndian>(segment)?
                }
            }
            Protocol::Tcp(port) => {
                w.write_all(encode::u32(TCP, &mut buf))?;
                w.write_u16::<BigEndian>(*port)?
            }
            Protocol::Udp(port) => {
                w.write_all(encode::u32(UDP, &mut buf))?;
                w.write_u16::<BigEndian>(*port)?
            }
            Protocol::Dccp(port) => {
                w.write_all(encode::u32(DCCP, &mut buf))?;
                w.write_u16::<BigEndian>(*port)?
            }
            Protocol::Sctp(port) => {
                w.write_all(encode::u32(SCTP, &mut buf))?;
                w.write_u16::<BigEndian>(*port)?
            }
            Protocol::Dns(s) => {
                w.write_all(encode::u32(DNS, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Dns4(s) => {
                w.write_all(encode::u32(DNS4, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Dns6(s) => {
                w.write_all(encode::u32(DNS6, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Dnsaddr(s) => {
                w.write_all(encode::u32(DNSADDR, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Unix(s) => {
                w.write_all(encode::u32(UNIX, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::P2p(multihash) => {
                w.write_all(encode::u32(P2P, &mut buf))?;
                let bytes = multihash.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Onion(addr, port) => {
                w.write_all(encode::u32(ONION, &mut buf))?;
                w.write_all(addr.as_ref())?;
                w.write_u16::<BigEndian>(*port)?
            }
            Protocol::Onion3(addr) => {
                w.write_all(encode::u32(ONION3, &mut buf))?;
                w.write_all(addr.hash().as_ref())?;
                w.write_u16::<BigEndian>(addr.port())?
            }
            Protocol::Quic => w.write_all(encode::u32(QUIC, &mut buf))?,
            Protocol::Utp => w.write_all(encode::u32(UTP, &mut buf))?,
            Protocol::Udt => w.write_all(encode::u32(UDT, &mut buf))?,
            Protocol::Http => w.write_all(encode::u32(HTTP, &mut buf))?,
            Protocol::Https => w.write_all(encode::u32(HTTPS, &mut buf))?,
            Protocol::Ws(ref s) if s == "/" => w.write_all(encode::u32(WS, &mut buf))?,
            Protocol::Ws(s) => {
                w.write_all(encode::u32(WS_WITH_PATH, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::Wss(ref s) if s == "/" => w.write_all(encode::u32(WSS, &mut buf))?,
            Protocol::Wss(s) => {
                w.write_all(encode::u32(WSS_WITH_PATH, &mut buf))?;
                let bytes = s.as_bytes();
                w.write_all(encode::usize(bytes.len(), &mut encode::usize_buffer()))?;
                w.write_all(&bytes)?
            }
            Protocol::P2pWebSocketStar => w.write_all(encode::u32(P2P_WEBSOCKET_STAR, &mut buf))?,
            Protocol::P2pWebRtcStar => w.write_all(encode::u32(P2P_WEBRTC_STAR, &mut buf))?,
            Protocol::P2pWebRtcDirect => w.write_all(encode::u32(P2P_WEBRTC_DIRECT, &mut buf))?,
            Protocol::P2pCircuit => w.write_all(encode::u32(P2P_CIRCUIT, &mut buf))?,
            Protocol::Memory(port) => {
                w.write_all(encode::u32(MEMORY, &mut buf))?;
                w.write_u64::<BigEndian>(*port)?
            }
        }
        Ok(())
    }

    /// Turn this `Protocol` into one that owns its data, thus being valid for any lifetime.
    pub fn acquire<'b>(self) -> Protocol<'b> {
        use self::Protocol::*;
        match self {
            Dccp(a) => Dccp(a),
            Dns(cow) => Dns(Cow::Owned(cow.into_owned())),
            Dns4(cow) => Dns4(Cow::Owned(cow.into_owned())),
            Dns6(cow) => Dns6(Cow::Owned(cow.into_owned())),
            Dnsaddr(cow) => Dnsaddr(Cow::Owned(cow.into_owned())),
            Http => Http,
            Https => Https,
            Ip4(a) => Ip4(a),
            Ip6(a) => Ip6(a),
            P2pWebRtcDirect => P2pWebRtcDirect,
            P2pWebRtcStar => P2pWebRtcStar,
            P2pWebSocketStar => P2pWebSocketStar,
            Memory(a) => Memory(a),
            Onion(addr, port) => Onion(Cow::Owned(addr.into_owned()), port),
            Onion3(addr) => Onion3(addr.acquire()),
            P2p(a) => P2p(a),
            P2pCircuit => P2pCircuit,
            Quic => Quic,
            Sctp(a) => Sctp(a),
            Tcp(a) => Tcp(a),
            Udp(a) => Udp(a),
            Udt => Udt,
            Unix(cow) => Unix(Cow::Owned(cow.into_owned())),
            Utp => Utp,
            Ws(cow) => Ws(Cow::Owned(cow.into_owned())),
            Wss(cow) => Wss(Cow::Owned(cow.into_owned())),
        }
    }
}

impl<'a> fmt::Display for Protocol<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::Protocol::*;
        match self {
            Dccp(port) => write!(f, "/dccp/{}", port),
            Dns(s) => write!(f, "/dns/{}", s),
            Dns4(s) => write!(f, "/dns4/{}", s),
            Dns6(s) => write!(f, "/dns6/{}", s),
            Dnsaddr(s) => write!(f, "/dnsaddr/{}", s),
            Http => f.write_str("/http"),
            Https => f.write_str("/https"),
            Ip4(addr) => write!(f, "/ip4/{}", addr),
            Ip6(addr) => write!(f, "/ip6/{}", addr),
            P2pWebRtcDirect => f.write_str("/p2p-webrtc-direct"),
            P2pWebRtcStar => f.write_str("/p2p-webrtc-star"),
            P2pWebSocketStar => f.write_str("/p2p-websocket-star"),
            Memory(port) => write!(f, "/memory/{}", port),
            Onion(addr, port) => {
                let s = BASE32.encode(addr.as_ref());
                write!(f, "/onion/{}:{}", s.to_lowercase(), port)
            }
            Onion3(addr) => {
                let s = BASE32.encode(addr.hash());
                write!(f, "/onion3/{}:{}", s.to_lowercase(), addr.port())
            }
            P2p(c) => write!(f, "/p2p/{}", bs58::encode(c.as_bytes()).into_string()),
            P2pCircuit => f.write_str("/p2p-circuit"),
            Quic => f.write_str("/quic"),
            Sctp(port) => write!(f, "/sctp/{}", port),
            Tcp(port) => write!(f, "/tcp/{}", port),
            Udp(port) => write!(f, "/udp/{}", port),
            Udt => f.write_str("/udt"),
            Unix(s) => write!(f, "/unix/{}", s),
            Utp => f.write_str("/utp"),
            Ws(ref s) if s == "/" => f.write_str("/ws"),
            Ws(s) => {
                let encoded = percent_encoding::percent_encode(s.as_bytes(), PATH_SEGMENT_ENCODE_SET);
                write!(f, "/x-parity-ws/{}", encoded)
            }
            Wss(ref s) if s == "/" => f.write_str("/wss"),
            Wss(s) => {
                let encoded = percent_encoding::percent_encode(s.as_bytes(), PATH_SEGMENT_ENCODE_SET);
                write!(f, "/x-parity-wss/{}", encoded)
            }
        }
    }
}

impl<'a> From<IpAddr> for Protocol<'a> {
    #[inline]
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(addr) => Protocol::Ip4(addr),
            IpAddr::V6(addr) => Protocol::Ip6(addr),
        }
    }
}

impl<'a> From<Ipv4Addr> for Protocol<'a> {
    #[inline]
    fn from(addr: Ipv4Addr) -> Self {
        Protocol::Ip4(addr)
    }
}

impl<'a> From<Ipv6Addr> for Protocol<'a> {
    #[inline]
    fn from(addr: Ipv6Addr) -> Self {
        Protocol::Ip6(addr)
    }
}

macro_rules! read_onion_impl {
    ($name:ident, $len:expr, $encoded_len:expr) => {
        fn $name(s: &str) -> Result<([u8; $len], u16)> {
            let mut parts = s.split(':');

            // address part (without ".onion")
            let b32 = parts.next().ok_or(Error::InvalidMultiaddr)?;
            if b32.len() != $encoded_len {
                return Err(Error::InvalidMultiaddr);
            }

            // port number
            let port = parts
                .next()
                .ok_or(Error::InvalidMultiaddr)
                .and_then(|p| str::parse(p).map_err(From::from))?;

            // port == 0 is not valid for onion
            if port == 0 {
                return Err(Error::InvalidMultiaddr);
            }

            // nothing else expected
            if parts.next().is_some() {
                return Err(Error::InvalidMultiaddr);
            }

            if $len != BASE32.decode_len(b32.len()).map_err(|_| Error::InvalidMultiaddr)? {
                return Err(Error::InvalidMultiaddr);
            }

            let mut buf = [0u8; $len];
            BASE32
                .decode_mut(b32.as_bytes(), &mut buf)
                .map_err(|_| Error::InvalidMultiaddr)?;

            Ok((buf, port))
        }
    };
}

// Parse a version 2 onion address and return its binary representation.
//
// Format: <base-32 address> ":" <port number>
read_onion_impl!(read_onion, 10, 16);
// Parse a version 3 onion address and return its binary representation.
//
// Format: <base-32 address> ":" <port number>
read_onion_impl!(read_onion3, 35, 56);
