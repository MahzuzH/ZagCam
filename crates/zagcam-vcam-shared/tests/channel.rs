//! Cross-thread smoke test for the frame channel: a producer and a consumer on the same
//! `Local\` section, checking that every successful read is self-consistent (no torn frames)
//! and that the consumer heartbeat reaches the producer side.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use zagcam_vcam_shared::{FrameChannel, nv12_len};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const FRAMES: u32 = 200;

fn encode(buf: &mut [u8], index: u32) {
    buf[..4].copy_from_slice(&index.to_le_bytes());
    buf[4..].fill((index & 0xFF) as u8);
}

fn decode(buf: &[u8]) -> Option<u32> {
    let index = u32::from_le_bytes(buf[..4].try_into().ok()?);
    buf[4..].iter().all(|&b| b == (index & 0xFF) as u8).then_some(index)
}

#[test]
fn producer_and_consumer_exchange_frames_without_tears() {
    let name = format!(r"Local\ZagCamSmoke-{}", std::process::id());
    let channel = Arc::new(FrameChannel::create_named(&name).expect("create channel"));
    let highest = Arc::new(AtomicU32::new(0));

    let producer = {
        let channel = Arc::clone(&channel);
        std::thread::spawn(move || {
            for index in 0..FRAMES {
                channel.write_frame(WIDTH, HEIGHT, |out| encode(out, index));
                std::thread::yield_now();
            }
        })
    };

    let consumer = {
        let channel = Arc::clone(&channel);
        let highest = Arc::clone(&highest);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; nv12_len(WIDTH, HEIGHT)];
            let mut tries = 0u32;
            loop {
                channel.announce_consumer(WIDTH, HEIGHT);
                if channel.read_frame(WIDTH, HEIGHT, &mut buf) {
                    let index = decode(&buf).expect("read returned a torn frame");
                    highest.fetch_max(index, Ordering::Relaxed);
                }
                tries += 1;
                if highest.load(Ordering::Relaxed) == FRAMES - 1 {
                    break;
                }
                assert!(tries < 200_000, "consumer never saw the last frame");
                std::thread::yield_now();
            }
            // The producer side must see our heartbeat.
            assert_eq!(channel.active_request(), Some((WIDTH, HEIGHT)));
        })
    };

    producer.join().expect("producer panicked");
    consumer.join().expect("consumer panicked");
    assert_eq!(highest.load(Ordering::Relaxed), FRAMES - 1);
}
