use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// A pool of reusable buffers to reduce memory allocations
/// This is particularly effective for encoding operations where
/// buffers are frequently allocated and deallocated
pub struct BufferPool {
    pool: Arc<Mutex<VecDeque<Vec<u8>>>>,
    max_pool_size: usize,
}

impl BufferPool {
    const DEFAULT_MAX_POOL_SIZE: usize = 64;
    const MIN_BUFFER_SIZE: usize = 4 * 1024; // 4KB
    const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MB

    pub fn new() -> Self {
        Self {
            pool: Arc::new(Mutex::new(VecDeque::new())),
            max_pool_size: Self::DEFAULT_MAX_POOL_SIZE,
        }
    }

    pub fn with_capacity(max_pool_size: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(VecDeque::with_capacity(max_pool_size))),
            max_pool_size,
        }
    }

    /// Get a buffer from the pool or create a new one
    pub fn get_buffer(&self, min_capacity: usize) -> PooledBuffer {
        let mut pool = self.pool.lock().unwrap();
        
        // Try to find a suitable buffer from the pool
        let mut buffer = pool
            .iter()
            .position(|buf| buf.capacity() >= min_capacity && buf.capacity() <= min_capacity * 2)
            .and_then(|index| pool.remove(index))
            .unwrap_or_else(|| {
                // Create new buffer if none suitable found
                Vec::with_capacity(min_capacity.max(Self::MIN_BUFFER_SIZE))
            });

        buffer.clear();
        
        PooledBuffer {
            buffer,
            pool: Arc::clone(&self.pool),
            max_pool_size: self.max_pool_size,
        }
    }
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

/// A buffer that automatically returns to the pool when dropped
pub struct PooledBuffer {
    buffer: Vec<u8>,
    pool: Arc<Mutex<VecDeque<Vec<u8>>>>,
    max_pool_size: usize,
}

impl PooledBuffer {
    /// Get the underlying buffer
    pub fn as_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    /// Get the underlying buffer
    pub fn as_ref(&self) -> &Vec<u8> {
        &self.buffer
    }

    /// Consume the pooled buffer and return the inner Vec
    pub fn into_inner(mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Return buffer to pool if it's a reasonable size and pool isn't full
        if self.buffer.capacity() >= BufferPool::MIN_BUFFER_SIZE 
            && self.buffer.capacity() <= BufferPool::MAX_BUFFER_SIZE {
            
            let mut pool = self.pool.lock().unwrap();
            if pool.len() < self.max_pool_size {
                self.buffer.clear();
                pool.push_back(std::mem::take(&mut self.buffer));
            }
        }
    }
}

impl std::ops::Deref for PooledBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl std::ops::DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

/// Global buffer pool instance
static GLOBAL_BUFFER_POOL: std::sync::OnceLock<BufferPool> = std::sync::OnceLock::new();

/// Get a buffer from the global pool
pub fn get_pooled_buffer(min_capacity: usize) -> PooledBuffer {
    let pool = GLOBAL_BUFFER_POOL.get_or_init(|| BufferPool::new());
    pool.get_buffer(min_capacity)
}
