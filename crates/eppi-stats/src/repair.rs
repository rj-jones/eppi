//! Fallback for replays peppi 2.1 refuses but slippi-js reads: files cut off mid-write (e.g.
//! Dolphin crashed) and Game End events with an end method peppi doesn't know (newer Slippi
//! versions write method 9).
//!
//! We walk the raw event stream using the payload-size table, cut a truncated stream after the
//! last complete frame, and swap an unknown end method for one peppi accepts. The caller restores
//! the original method byte in `game.end.bytes` after parsing.

/// `{U\x03raw[$U#l`: the UBJSON prefix of every replay, followed by the raw length (u32 BE).
const SIGNATURE_LEN: usize = 11;
const RAW_START: usize = SIGNATURE_LEN + 4;

const EVENT_PAYLOADS: u8 = 0x35;
const EVENT_GAME_END: u8 = 0x39;
const EVENT_FRAME_END: u8 = 0x3c;
/// End methods peppi 2.1 accepts.
const KNOWN_END_METHODS: [u8; 5] = [0, 1, 2, 3, 7];
/// Stand-in for unknown end methods while parsing (Resolved).
const STAND_IN_END_METHOD: u8 = 3;

/// A rewritten replay.
pub(crate) struct Repaired {
    /// Bytes to parse.
    pub bytes: Vec<u8>,
    /// Original end method, if it was swapped for parsing.
    pub original_end_method: Option<u8>,
}

/// Returns a parseable copy of `data`, or `None` if there's nothing this can fix.
pub(crate) fn repair(data: &[u8]) -> Option<Repaired> {
    let raw_len = u32::from_be_bytes(data.get(SIGNATURE_LEN..RAW_START)?.try_into().ok()?) as usize;
    let declared_end = RAW_START.checked_add(raw_len)?;
    let truncated = raw_len == 0 || declared_end > data.len();
    let raw_end = if truncated { data.len() } else { declared_end };

    // Payload sizes.
    if *data.get(RAW_START)? != EVENT_PAYLOADS {
        return None;
    }
    let table_size = *data.get(RAW_START + 1)? as usize;
    let table = data.get(RAW_START + 2..RAW_START + 1 + table_size)?;
    let mut sizes = [0usize; 256];
    for &[code, hi, lo] in table.as_chunks::<3>().0 {
        sizes[code as usize] = u16::from_be_bytes([hi, lo]) as usize;
    }

    // Walk events.
    let mut pos = RAW_START + 1 + table_size;
    let mut last_complete_event = pos;
    let mut last_frame_end = None;
    let mut game_end_at = None;
    while pos < raw_end {
        let code = data[pos];
        let size = sizes[code as usize];
        if size == 0 || pos + 1 + size > raw_end {
            break;
        }
        let next = pos + 1 + size;
        last_complete_event = next;
        match code {
            EVENT_FRAME_END => last_frame_end = Some(next),
            EVENT_GAME_END => {
                game_end_at = Some(pos);
                break;
            }
            _ => {}
        }
        pos = next;
    }

    let mut original_end_method = None;
    let mut bytes = if truncated {
        // Keep whole frames (whole events before Slippi 3.0, which has no Frame End), plus a
        // Game End if one made it in. Metadata is dropped.
        let cut = match (game_end_at, last_frame_end) {
            (Some(_), _) | (None, None) => last_complete_event,
            (None, Some(frame_end)) => frame_end,
        };
        let mut out = Vec::with_capacity(cut + 1);
        out.extend_from_slice(&data[..SIGNATURE_LEN]);
        out.extend_from_slice(&u32::try_from(cut - RAW_START).ok()?.to_be_bytes());
        out.extend_from_slice(&data[RAW_START..cut]);
        out.push(b'}');
        out
    } else {
        data.to_vec()
    };

    if let Some(at) = game_end_at {
        let method = bytes.get_mut(at + 1)?;
        if !KNOWN_END_METHODS.contains(method) {
            original_end_method = Some(*method);
            *method = STAND_IN_END_METHOD;
        }
    }

    (truncated || original_end_method.is_some()).then_some(Repaired {
        bytes,
        original_end_method,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal stream: payload sizes (Game Start 2 bytes, Frame End 1, Game End 2), a
    /// Game Start, two Frame Ends and a Game End.
    fn replay(end_method: u8) -> Vec<u8> {
        let mut raw = vec![
            EVENT_PAYLOADS,
            10,
            0x36,
            0,
            2,
            EVENT_FRAME_END,
            0,
            1,
            EVENT_GAME_END,
            0,
            2,
        ];
        raw.extend([0x36, 0, 0]);
        raw.extend([EVENT_FRAME_END, 0]);
        raw.extend([EVENT_FRAME_END, 0]);
        raw.extend([EVENT_GAME_END, end_method, 0xff]);
        let mut out = b"{U\x03raw[$U#l".to_vec();
        out.extend((raw.len() as u32).to_be_bytes());
        out.extend(raw);
        out.extend(b"}");
        out
    }

    #[test]
    fn leaves_good_replays_alone() {
        assert!(repair(&replay(2)).is_none());
    }

    #[test]
    fn swaps_unknown_end_method() {
        let data = replay(9);
        let r = repair(&data).unwrap();
        assert_eq!(r.original_end_method, Some(9));
        assert_eq!(r.bytes.len(), data.len());
        assert_eq!(r.bytes[data.len() - 3], STAND_IN_END_METHOD);
    }

    #[test]
    fn cuts_truncated_stream_after_last_frame() {
        let full = replay(2);
        // Drop the Game End's last byte and the closing brace; zero the raw length.
        let mut data = full[..full.len() - 2].to_vec();
        data[SIGNATURE_LEN..RAW_START].copy_from_slice(&[0; 4]);
        let r = repair(&data).unwrap();
        // Everything up to and including the second Frame End, then "}".
        let expected_raw_len = full.len() - 1 - 3 - RAW_START;
        assert_eq!(
            u32::from_be_bytes(r.bytes[SIGNATURE_LEN..RAW_START].try_into().unwrap()) as usize,
            expected_raw_len
        );
        assert_eq!(r.bytes.len(), RAW_START + expected_raw_len + 1);
        assert_eq!(*r.bytes.last().unwrap(), b'}');
    }
}
