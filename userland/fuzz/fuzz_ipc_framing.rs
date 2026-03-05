#![no_main]

use agnos_common::Message;
use libfuzzer_sys::fuzz_target;

/// Fuzz IPC length-prefixed framing: try to parse arbitrary bytes as
/// a 4-byte big-endian length header followed by a JSON Message body.
/// This exercises the same code path as `handle_connection` without
/// requiring a live socket.
fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // Extract length prefix
    let len_bytes: [u8; 4] = [data[0], data[1], data[2], data[3]];
    let msg_len = u32::from_be_bytes(len_bytes);

    // Reject oversized (matches MAX_MESSAGE_SIZE = 64 KB)
    if msg_len > 64 * 1024 || msg_len == 0 {
        return;
    }

    let body = &data[4..];
    if (body.len() as u32) < msg_len {
        return;
    }

    let payload = &body[..msg_len as usize];

    // Try to deserialize — must not panic
    let _ = serde_json::from_slice::<Message>(payload);

    // Try round-trip: serialize then deserialize
    if let Ok(msg) = serde_json::from_slice::<Message>(payload) {
        let serialized = serde_json::to_vec(&msg).unwrap();
        let _roundtrip: Message = serde_json::from_slice(&serialized).unwrap();
    }
});
