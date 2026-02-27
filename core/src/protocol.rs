//! Protocol: Frame codec and typed Message enum.

use bytes::{Buf, BufMut, BytesMut};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u8 = 0;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamId {
    Control = 1,
    Clipboard = 2,
    File = 3,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    Hello = 1,
    Ping = 2,
    Pong = 3,
    ClipText = 10,
    ClipImage = 11,
    FileOffer = 20,
    FileAccept = 21,
    FileReject = 22,
    FileChunk = 23,
    FileDone = 24,
}

impl MsgType {
    pub fn from_u8(v: u8) -> anyhow::Result<Self> {
        match v {
            1 => Ok(Self::Hello),
            2 => Ok(Self::Ping),
            3 => Ok(Self::Pong),
            10 => Ok(Self::ClipText),
            11 => Ok(Self::ClipImage),
            20 => Ok(Self::FileOffer),
            21 => Ok(Self::FileAccept),
            22 => Ok(Self::FileReject),
            23 => Ok(Self::FileChunk),
            24 => Ok(Self::FileDone),
            _ => anyhow::bail!("unknown MsgType: {v}"),
        }
    }
}

/// Wire frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub version: u8,
    pub msg_type: u8,
    pub stream_id: u32,
    pub seq: u64,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(msg_type: MsgType, stream_id: StreamId, seq: u64, payload: Vec<u8>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            msg_type: msg_type as u8,
            stream_id: stream_id as u32,
            seq,
            payload,
        }
    }
}

pub fn encode_frame(f: &Frame) -> Vec<u8> {
    let mut b = BytesMut::with_capacity(18 + f.payload.len());
    b.put_u8(f.version);
    b.put_u8(f.msg_type);
    b.put_u32(f.stream_id);
    b.put_u64(f.seq);
    b.put_u32(f.payload.len() as u32);
    b.extend_from_slice(&f.payload);
    b.to_vec()
}

pub fn decode_frame(mut bytes: &[u8]) -> anyhow::Result<Frame> {
    if bytes.len() < 18 {
        anyhow::bail!("insufficient data");
    }
    let version = bytes.get_u8();
    let msg_type = bytes.get_u8();
    let stream_id = bytes.get_u32();
    let seq = bytes.get_u64();
    let len = bytes.get_u32() as usize;
    if bytes.len() < len {
        anyhow::bail!("payload truncated");
    }
    let payload = bytes[..len].to_vec();
    Ok(Frame { version, msg_type, stream_id, seq, payload })
}

/// Typed application messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Message {
    Hello { peer_id: String, version: u8 },
    Ping { ts_ms: u64 },
    Pong { ts_ms: u64 },
    ClipText { mime: String, text: String, ts_ms: u64 },
    ClipImage { mime: String, width: u32, height: u32, bytes_b64: String, ts_ms: u64 },
    FileOffer { file_id: String, name: String, size: u64, mime: String },
    FileAccept { file_id: String },
    FileReject { file_id: String, reason: String },
    FileChunk { file_id: String, offset: u64, data_b64: String },
    FileDone { file_id: String, hash: String },
}

impl Message {
    pub fn msg_type(&self) -> MsgType {
        match self {
            Self::Hello { .. } => MsgType::Hello,
            Self::Ping { .. } => MsgType::Ping,
            Self::Pong { .. } => MsgType::Pong,
            Self::ClipText { .. } => MsgType::ClipText,
            Self::ClipImage { .. } => MsgType::ClipImage,
            Self::FileOffer { .. } => MsgType::FileOffer,
            Self::FileAccept { .. } => MsgType::FileAccept,
            Self::FileReject { .. } => MsgType::FileReject,
            Self::FileChunk { .. } => MsgType::FileChunk,
            Self::FileDone { .. } => MsgType::FileDone,
        }
    }

    pub fn stream_id(&self) -> StreamId {
        match self {
            Self::Hello { .. } | Self::Ping { .. } | Self::Pong { .. } => StreamId::Control,
            Self::ClipText { .. } | Self::ClipImage { .. } => StreamId::Clipboard,
            Self::FileOffer { .. } | Self::FileAccept { .. } | Self::FileReject { .. }
            | Self::FileChunk { .. } | Self::FileDone { .. } => StreamId::File,
        }
    }
}

pub fn encode_message(msg: &Message, seq: u64) -> anyhow::Result<Vec<u8>> {
    let payload = serde_json::to_vec(msg)?;
    let frame = Frame::new(msg.msg_type(), msg.stream_id(), seq, payload);
    Ok(encode_frame(&frame))
}

pub fn decode_message(bytes: &[u8]) -> anyhow::Result<(Message, u64)> {
    let frame = decode_frame(bytes)?;
    let msg: Message = serde_json::from_slice(&frame.payload)?;
    Ok((msg, frame.seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(msg: Message) {
        let enc = encode_message(&msg, 1).unwrap();
        let (dec, seq) = decode_message(&enc).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(dec, msg);
    }

    #[test]
    fn roundtrip_hello() { roundtrip(Message::Hello { peer_id: "abc".into(), version: 0 }); }
    #[test]
    fn roundtrip_ping() { roundtrip(Message::Ping { ts_ms: 123 }); }
    #[test]
    fn roundtrip_pong() { roundtrip(Message::Pong { ts_ms: 456 }); }
    #[test]
    fn roundtrip_clip_text() { roundtrip(Message::ClipText { mime: "text/plain".into(), text: "hello".into(), ts_ms: 1 }); }
    #[test]
    fn roundtrip_clip_image() { roundtrip(Message::ClipImage { mime: "image/png".into(), width: 10, height: 10, bytes_b64: "AAAA".into(), ts_ms: 2 }); }
    #[test]
    fn roundtrip_file_offer() { roundtrip(Message::FileOffer { file_id: "f1".into(), name: "a.txt".into(), size: 100, mime: "text/plain".into() }); }
    #[test]
    fn roundtrip_file_accept() { roundtrip(Message::FileAccept { file_id: "f1".into() }); }
    #[test]
    fn roundtrip_file_reject() { roundtrip(Message::FileReject { file_id: "f1".into(), reason: "no".into() }); }
    #[test]
    fn roundtrip_file_chunk() { roundtrip(Message::FileChunk { file_id: "f1".into(), offset: 0, data_b64: "AQID".into() }); }
    #[test]
    fn roundtrip_file_done() { roundtrip(Message::FileDone { file_id: "f1".into(), hash: "abc123".into() }); }

    #[test]
    fn frame_roundtrip() {
        let f = Frame::new(MsgType::Ping, StreamId::Control, 42, b"hi".to_vec());
        let enc = encode_frame(&f);
        let dec = decode_frame(&enc).unwrap();
        assert_eq!(dec, f);
    }
}
