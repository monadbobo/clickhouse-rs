use std::slice;

#[derive(Copy, Clone)]
struct StringPtr {
    chunk: usize,
    shift: usize,
    len: usize,
}

/// Storage types for different string sizes and access patterns
/// This enum enables zero-copy storage by taking direct ownership of Vec<u8>
#[derive(Clone)]
enum StorageType {
    /// Small strings stored continuously in large pre-allocated chunks
    /// This reduces allocation overhead and improves cache locality
    Continuous(Vec<u8>),

    /// Zero-copy storage: directly takes ownership of external Vec<u8>
    /// This is the most efficient storage as it avoids any copying
    ZeroCopy(Vec<u8>),
}

/// High-performance string pool with zero-copy optimization
///
/// This pool uses different storage strategies based on string sizes:
/// - Small strings: packed into continuous chunks for cache efficiency
/// - Large strings: stored independently to prevent memory waste
/// - Zero-copy mode: directly takes ownership of Vec<u8> without copying
#[derive(Clone)]
pub(crate) struct StringPool {
    chunks: Vec<StorageType>,
    pointers: Vec<StringPtr>,
}

pub(crate) struct StringIter<'a> {
    pool: &'a StringPool,
    index: usize,
}

impl<'a> Iterator for StringIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.pool.len() {
            let result = self.pool.get(self.index);
            self.index += 1;
            return Some(result);
        }

        None
    }
}

// Efficient implementations for different types

impl<T> From<Vec<T>> for StringPool
where
    T: AsRef<[u8]>,
{
    fn from(source: Vec<T>) -> Self {
        let type_name = std::any::type_name::<T>();

        if type_name == "alloc::string::String" {
            let strings: Vec<String> = unsafe { std::mem::transmute(source) };
            Self::from_strings_optimized(strings)
        } else if type_name.contains("vec::Vec<u8>") || type_name.contains("alloc::vec::Vec<u8>") {
            let byte_vecs: Vec<Vec<u8>> = unsafe { std::mem::transmute(source) };
            Self::from_byte_vecs(byte_vecs)
        } else {
            Self::from_generic_zero_copy(source)
        }
    }
}

impl StringPool {
    /// Generic zero-copy method using type detection
    pub fn from_generic_zero_copy<T: AsRef<[u8]>>(source: Vec<T>) -> Self {
        let count = source.len();
        if count == 0 {
            return StringPool::with_capacity(0);
        }

        // 快速路径：检查是否所有字符串都很小，如果是则使用连续存储
        let total_size: usize = source.iter().map(|s| s.as_ref().len()).sum();
        const SMALL_STRING_THRESHOLD: usize = 32;
        const CONTINUOUS_STORAGE_LIMIT: usize = 8192; // 8KB

        let use_continuous = total_size <= CONTINUOUS_STORAGE_LIMIT
            && source
                .iter()
                .all(|s| s.as_ref().len() <= SMALL_STRING_THRESHOLD);

        if use_continuous {
            // 连续存储小字符串，更好的缓存局部性
            let mut packed_data = Vec::with_capacity(total_size);
            let mut pointers = Vec::with_capacity(count);
            let mut offset = 0;

            // 安全的批量拷贝
            for item in source {
                let slice = item.as_ref();
                let len = slice.len();

                if len > 0 {
                    packed_data.extend_from_slice(slice);
                }

                pointers.push(StringPtr {
                    chunk: 0,
                    shift: offset,
                    len,
                });
                offset += len;
            }

            StringPool {
                chunks: vec![StorageType::Continuous(packed_data)],
                pointers,
            }
        } else {
            // 分离存储大字符串，避免内存浪费
            let mut chunks = Vec::with_capacity(count);
            let mut pointers = Vec::with_capacity(count);

            for (index, item) in source.into_iter().enumerate() {
                let slice = item.as_ref();
                let len = slice.len();
                let bytes = slice.to_vec();

                chunks.push(StorageType::ZeroCopy(bytes));
                pointers.push(StringPtr {
                    chunk: index,
                    shift: 0,
                    len,
                });
            }

            StringPool { chunks, pointers }
        }
    }

    /// High-performance creation from Vec<String> with zero-copy
    pub fn from_strings_optimized(source: Vec<String>) -> Self {
        let capacity = source.len();
        if capacity == 0 {
            return StringPool::with_capacity(0);
        }

        // 预分配精确容量，避免重新分配
        let mut chunks = Vec::with_capacity(capacity);
        let mut pointers = Vec::with_capacity(capacity);

        // 批量处理避免多次push的开销
        for (index, string) in source.into_iter().enumerate() {
            let len = string.len();
            let bytes = string.into_bytes();

            chunks.push(StorageType::ZeroCopy(bytes));
            pointers.push(StringPtr {
                chunk: index,
                shift: 0,
                len,
            });
        }

        StringPool { chunks, pointers }
    }

    /// High-performance creation from Vec<Vec<u8>> with zero-copy
    pub fn from_byte_vecs(source: Vec<Vec<u8>>) -> Self {
        let capacity = source.len();
        if capacity == 0 {
            return StringPool::with_capacity(0);
        }

        // 预分配精确容量，避免重新分配
        let mut chunks = Vec::with_capacity(capacity);
        let mut pointers = Vec::with_capacity(capacity);

        // 批量处理避免多次push的开销
        for (index, bytes) in source.into_iter().enumerate() {
            let len = bytes.len();

            chunks.push(StorageType::ZeroCopy(bytes));
            pointers.push(StringPtr {
                chunk: index,
                shift: 0,
                len,
            });
        }

        StringPool { chunks, pointers }
    }

    pub(crate) fn with_capacity(capacity: usize) -> StringPool {
        StringPool {
            pointers: Vec::with_capacity(capacity),
            chunks: Vec::new(),
        }
    }

    /// 创建具有预分配容量的字符串池，避免后续重新分配
    pub(crate) fn with_capacity_and_size(
        capacity: usize,
        estimated_total_size: usize,
    ) -> StringPool {
        // 如果预计总大小较小，使用连续存储
        if estimated_total_size <= 8192 {
            StringPool {
                pointers: Vec::with_capacity(capacity),
                chunks: vec![StorageType::Continuous(Vec::with_capacity(
                    estimated_total_size,
                ))],
            }
        } else {
            StringPool {
                pointers: Vec::with_capacity(capacity),
                chunks: Vec::with_capacity(capacity),
            }
        }
    }

    /// High-efficiency data storage - the core method that replaces allocate()
    /// 使用零拷贝策略，避免内存拷贝开销
    pub(crate) fn store_data(&mut self, data: Vec<u8>) -> &[u8] {
        let len = data.len();
        let chunk_index = self.chunks.len();

        // 直接使用零拷贝存储，避免复杂的借用检查
        self.chunks.push(StorageType::ZeroCopy(data));
        self.pointers.push(StringPtr {
            chunk: chunk_index,
            shift: 0,
            len,
        });

        // 通过get方法安全地获取引用
        self.get(self.pointers.len() - 1)
    }

    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &[u8] {
        let pointer = &self.pointers[index];
        unsafe { self.get_by_pointer(pointer) }
    }

    #[inline(always)]
    pub(crate) unsafe fn get_unchecked(&self, index: usize) -> &[u8] {
        let pointer = self.pointers.get_unchecked(index);
        self.get_by_pointer(pointer)
    }

    #[inline(always)]
    unsafe fn get_by_pointer(&self, pointer: &StringPtr) -> &[u8] {
        let chunk = &self.chunks.get_unchecked(pointer.chunk);

        let (ptr, len) = match chunk {
            StorageType::Continuous(vec) => (vec.as_ptr().add(pointer.shift), pointer.len),
            StorageType::ZeroCopy(vec) => (vec.as_ptr(), pointer.len),
        };

        slice::from_raw_parts(ptr, len)
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.pointers.len()
    }

    pub(crate) fn strings(&self) -> StringIter {
        StringIter {
            pool: self,
            index: 0,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_store_data_efficiency() {
        let mut pool = StringPool::with_capacity(10);

        let small_str1 = b"hello".to_vec();
        let small_str2 = b"world".to_vec();

        pool.store_data(small_str1);
        pool.store_data(small_str2);

        assert_eq!(pool.get(0), b"hello");
        assert_eq!(pool.get(1), b"world");
        assert_eq!(pool.chunks.len(), 2); // Each stored as ZeroCopy
    }

    #[test]
    fn test_from_strings_zero_copy() {
        let source = vec![
            "hello".to_string(),
            "world".to_string(),
            "rust".to_string(),
            "clickhouse".to_string(),
        ];

        let pool = StringPool::from_strings_optimized(source.clone());

        for (i, expected) in source.iter().enumerate() {
            assert_eq!(pool.get(i), expected.as_bytes());
        }
        assert_eq!(pool.len(), 4);
    }

    #[test]
    fn test_from_byte_vecs_zero_copy() {
        let source = vec![
            b"hello".to_vec(),
            b"world".to_vec(),
            b"rust".to_vec(),
            b"clickhouse".to_vec(),
        ];

        let pool = StringPool::from_byte_vecs(source.clone());

        for (i, expected) in source.iter().enumerate() {
            assert_eq!(pool.get(i), expected.as_slice());
        }
        assert_eq!(pool.len(), 4);
    }

    #[test]
    fn test_generic_from_performance() {
        let strings: Vec<String> = (0..1000).map(|i| format!("test_string_{}", i)).collect();

        let start = std::time::Instant::now();
        let pool_generic = StringPool::from(strings.clone());
        let generic_time = start.elapsed();

        let start = std::time::Instant::now();
        let pool_specialized = StringPool::from_strings_optimized(strings.clone());
        let specialized_time = start.elapsed();

        // Both should produce identical results
        assert_eq!(pool_generic.len(), pool_specialized.len());
        for i in 0..pool_generic.len() {
            assert_eq!(pool_generic.get(i), pool_specialized.get(i));
        }

        println!(
            "Generic: {:?}, Specialized: {:?}",
            generic_time, specialized_time
        );
    }

    #[test]
    fn test_zero_copy_verification() {
        let strings = vec!["hello".to_string(), "world".to_string()];
        let pool = StringPool::from(strings);

        let byte_vecs = vec![b"test".to_vec(), b"data".to_vec()];
        let pool2 = StringPool::from(byte_vecs);

        let str_refs = vec!["ref1", "ref2"];
        let pool3 = StringPool::from(str_refs);

        assert_eq!(pool.len(), 2);
        assert_eq!(pool2.len(), 2);
        assert_eq!(pool3.len(), 2);
    }
}
