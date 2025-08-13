use crate::{
    binary,
    types::{Marshal, StatBuffer},
};

const MAX_VARINT_LEN64: usize = 10;
const DEFAULT_BUFFER_CAPACITY: usize = 64 * 1024; // 64KB default buffer
const GROWTH_THRESHOLD: usize = 8 * 1024; // 8KB threshold for smart growth

/// High-performance encoder with optimized buffer management
/// Reduces memory allocations by pre-allocating large buffers and smart growth
#[derive(Default)]
pub struct Encoder {
    buffer: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder { 
            buffer: Vec::with_capacity(DEFAULT_BUFFER_CAPACITY) 
        }
    }

    /// Create encoder with specific capacity hint
    pub fn with_capacity(capacity: usize) -> Self {
        Encoder {
            buffer: Vec::with_capacity(capacity.max(DEFAULT_BUFFER_CAPACITY))
        }
    }

    pub fn uvarint(&mut self, v: u64) {
        let mut scratch = [0u8; MAX_VARINT_LEN64];
        let ln = binary::put_uvarint(&mut scratch[..], v);
        self.write_bytes_unchecked(&scratch[..ln]);
    }

    pub fn string(&mut self, text: impl AsRef<str>) {
        let bytes = text.as_ref().as_bytes();
        self.byte_string(bytes);
    }

    pub fn byte_string(&mut self, source: impl AsRef<[u8]>) {
        let src_bytes = source.as_ref();
        self.uvarint(src_bytes.len() as u64);
        self.write_bytes_unchecked(src_bytes);
    }

    pub fn write<T>(&mut self, value: T)
    where
        T: Copy + Marshal + StatBuffer,
    {
        let mut buffer = T::buffer();
        value.marshal(buffer.as_mut());
        self.write_bytes_unchecked(buffer.as_ref());
    }

    /// Optimized write_bytes with smart capacity management
    #[inline(always)]
    pub fn write_bytes(&mut self, b: &[u8]) {
        let needed_capacity = self.buffer.len() + b.len();
        
        // Smart growth: only reserve if we need significantly more space
        if needed_capacity > self.buffer.capacity() {
            let new_capacity = if b.len() > GROWTH_THRESHOLD {
                // For large writes, reserve exactly what we need plus some buffer
                needed_capacity + DEFAULT_BUFFER_CAPACITY
            } else {
                // For small writes, double the capacity to reduce future allocations
                self.buffer.capacity().max(DEFAULT_BUFFER_CAPACITY) * 2
            };
            self.buffer.reserve(new_capacity - self.buffer.capacity());
        }
        
        self.buffer.extend_from_slice(b);
    }

    /// Unchecked version for internal use where we know capacity is sufficient
    #[inline(always)]
    fn write_bytes_unchecked(&mut self, b: &[u8]) {
        // For small, known-size writes (like varints), just extend
        if b.len() <= MAX_VARINT_LEN64 {
            self.buffer.extend_from_slice(b);
        } else {
            self.write_bytes(b);
        }
    }

    pub fn get_buffer(self) -> Vec<u8> {
        self.buffer
    }

    pub fn get_buffer_ref(&self) -> &[u8] {
        self.buffer.as_ref()
    }
}
