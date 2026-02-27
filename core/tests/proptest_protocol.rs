use openclipboard_core::protocol::{decode_frame, decode_message, encode_message, Message};

use proptest::prelude::*;
use std::panic::catch_unwind;

proptest! {
    #[test]
    fn decode_frame_never_panics(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = catch_unwind(|| {
            let _ = decode_frame(&data);
        }).expect("decode_frame panicked");
    }

    #[test]
    fn message_encode_decode_roundtrip(msg in arb_message()) {
        let enc = encode_message(&msg, 123).expect("encode_message");
        let (dec, seq) = decode_message(&enc).expect("decode_message");
        prop_assert_eq!(seq, 123);
        prop_assert_eq!(dec, msg);
    }
}

fn arb_message() -> impl Strategy<Value = Message> {
    let small_string = "[ -~]{0,128}"; // printable ASCII, small

    prop_oneof![
        (small_string, any::<u8>(), small_string, small_string, small_string).prop_map(
            |(peer_id, version, identity_pk_b64, nonce_b64, sig_b64)| Message::Hello {
                peer_id,
                version,
                identity_pk_b64,
                nonce_b64,
                sig_b64,
            }
        ),
        any::<u64>().prop_map(|ts_ms| Message::Ping { ts_ms }),
        any::<u64>().prop_map(|ts_ms| Message::Pong { ts_ms }),
        (small_string, small_string, any::<u64>()).prop_map(|(mime, text, ts_ms)| Message::ClipText { mime, text, ts_ms }),
        (small_string, any::<u32>(), any::<u32>(), small_string, any::<u64>()).prop_map(
            |(mime, width, height, bytes_b64, ts_ms)| Message::ClipImage { mime, width, height, bytes_b64, ts_ms }
        ),
        (small_string, small_string, any::<u64>(), small_string).prop_map(
            |(file_id, name, size, mime)| Message::FileOffer { file_id, name, size, mime }
        ),
        small_string.prop_map(|file_id| Message::FileAccept { file_id }),
        (small_string, small_string).prop_map(|(file_id, reason)| Message::FileReject { file_id, reason }),
        (small_string, any::<u64>(), small_string).prop_map(
            |(file_id, offset, data_b64)| Message::FileChunk { file_id, offset, data_b64 }
        ),
        (small_string, small_string).prop_map(|(file_id, hash)| Message::FileDone { file_id, hash }),
    ]
}
