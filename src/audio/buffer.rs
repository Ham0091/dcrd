use crossbeam_queue::ArrayQueue;
use std::sync::Arc;

/// Lock-free SPSC (Single-Producer, Single-Consumer) ring buffer for audio samples.
///
/// Uses `crossbeam_queue::ArrayQueue` which is lock-free and wait-free
/// for both push and pop operations.
///
/// Capacity is in samples (i16), not bytes.
pub type AudioBuffer = Arc<ArrayQueue<i16>>;

/// Create a new audio ring buffer with the given capacity in samples.
///
/// For Opus frames at 48kHz mono:
/// - 960 samples per frame (20ms)
/// - Triple-buffered: 960 × 3 = 2880 samples minimum
/// - Recommended: 960 × 8 = 7680 samples for jitter tolerance
#[allow(dead_code)]
pub fn new_buffer(capacity: usize) -> AudioBuffer {
    Arc::new(ArrayQueue::new(capacity))
}

/// Number of samples per Opus frame.
#[allow(dead_code)]
pub const FRAME_SAMPLES: usize = 960;

/// Default ring buffer capacity (8 frames of jitter tolerance).
#[allow(dead_code)]
pub const DEFAULT_CAPACITY: usize = FRAME_SAMPLES * 8;
