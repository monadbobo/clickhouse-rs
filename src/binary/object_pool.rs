use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use crate::binary::Encoder;

/// Object pool for reusing expensive-to-create objects
pub struct ObjectPool<T> {
    objects: Arc<Mutex<VecDeque<T>>>,
    max_size: usize,
    factory: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T> ObjectPool<T> 
where 
    T: Send + 'static,
{
    pub fn new<F>(max_size: usize, factory: F) -> Self 
    where 
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            objects: Arc::new(Mutex::new(VecDeque::new())),
            max_size,
            factory: Box::new(factory),
        }
    }

    pub fn get(&self) -> PooledObject<T> {
        let mut objects = self.objects.lock().unwrap();
        let object = objects.pop_front().unwrap_or_else(|| (self.factory)());
        
        PooledObject {
            object: Some(object),
            pool: Arc::clone(&self.objects),
            max_size: self.max_size,
        }
    }
}

pub struct PooledObject<T> {
    object: Option<T>,
    pool: Arc<Mutex<VecDeque<T>>>,
    max_size: usize,
}

impl<T> PooledObject<T> {
    pub fn as_mut(&mut self) -> &mut T {
        self.object.as_mut().expect("Object already consumed")
    }

    pub fn as_ref(&self) -> &T {
        self.object.as_ref().expect("Object already consumed")
    }

    pub fn take(mut self) -> T {
        self.object.take().expect("Object already consumed")
    }
}

impl<T> Drop for PooledObject<T> {
    fn drop(&mut self) {
        if let Some(object) = self.object.take() {
            let mut pool = self.pool.lock().unwrap();
            if pool.len() < self.max_size {
                pool.push_back(object);
            }
        }
    }
}

impl<T> std::ops::Deref for PooledObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.object.as_ref().expect("Object already consumed")
    }
}

impl<T> std::ops::DerefMut for PooledObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.object.as_mut().expect("Object already consumed")
    }
}

/// Trait for objects that can be reset for reuse
pub trait Resettable {
    fn reset(&mut self);
}

impl Resettable for Encoder {
    fn reset(&mut self) {
        // Access buffer field directly since we can't make it public
        // For now, just create a new encoder
        *self = Encoder::new();
    }
}

/// Global encoder pool
static ENCODER_POOL: std::sync::OnceLock<ObjectPool<Encoder>> = std::sync::OnceLock::new();

pub fn get_pooled_encoder() -> PooledObject<Encoder> {
    let pool = ENCODER_POOL.get_or_init(|| {
        ObjectPool::new(16, || Encoder::new()) // Pool of 16 encoders
    });
    
    let mut encoder = pool.get();
    encoder.reset(); // Reset for reuse
    encoder
}
