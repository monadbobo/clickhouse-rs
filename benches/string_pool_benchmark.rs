use clickhouse_rs::types::column::string_pool::StringPool;
use std::time::Instant;

fn main() {
    println!("StringPool 性能对比测试");
    println!("======================");

    // 测试数据准备
    let test_sizes = vec![1000, 10000, 100000];

    for &size in &test_sizes {
        println!("\n测试大小: {} 个字符串", size);

        // 准备测试数据
        let strings: Vec<String> = (0..size)
            .map(|i| format!("test_string_number_{}_with_some_content", i))
            .collect();

        // 测试1: 使用 From<Vec<String>>（会产生拷贝）
        let strings_clone = strings.clone();
        let start = Instant::now();
        let _pool1 = StringPool::from(strings_clone);
        let from_trait_time = start.elapsed();

        // 测试2: 使用 from_strings（零拷贝）
        let strings_move = strings.clone();
        let start = Instant::now();
        let _pool2 = StringPool::from_strings(strings_move);
        let from_strings_time = start.elapsed();

        println!("  From trait:     {:?}", from_trait_time);
        println!("  from_strings:   {:?}", from_strings_time);
        println!(
            "  性能提升:       {:.2}x",
            from_trait_time.as_nanos() as f64 / from_strings_time.as_nanos() as f64
        );
    }

    // 大字符串测试
    println!("\n大字符串测试 (1MB each):");
    let large_strings: Vec<String> = (0..100)
        .map(|i| format!("{}{}", "x".repeat(1024 * 1024), i))
        .collect();

    let large_clone = large_strings.clone();
    let start = Instant::now();
    let _pool1 = StringPool::from(large_clone);
    let from_trait_large = start.elapsed();

    let large_move = large_strings;
    let start = Instant::now();
    let _pool2 = StringPool::from_strings(large_move);
    let from_strings_large = start.elapsed();

    println!("  From trait:     {:?}", from_trait_large);
    println!("  from_strings:   {:?}", from_strings_large);
    println!(
        "  性能提升:       {:.2}x",
        from_trait_large.as_nanos() as f64 / from_strings_large.as_nanos() as f64
    );
}

#[cfg(test)]
mod benches {
    use super::*;

    #[test]
    fn benchmark_small_strings() {
        let strings: Vec<String> = (0..1000).map(|i| format!("test_{}", i)).collect();

        // 确保两种方法产生相同结果
        let pool1 = StringPool::from(strings.clone());
        let pool2 = StringPool::from_strings(strings.clone());

        assert_eq!(pool1.len(), pool2.len());
        for i in 0..pool1.len() {
            assert_eq!(pool1.get(i), pool2.get(i));
        }
    }

    #[test]
    fn benchmark_memory_usage() {
        use std::mem;

        let strings: Vec<String> = (0..1000)
            .map(|i| format!("memory_test_string_number_{}", i))
            .collect();

        // 计算原始数据大小
        let original_size: usize = strings
            .iter()
            .map(|s| s.len() + mem::size_of::<String>())
            .sum();

        let pool = StringPool::from_strings(strings);
        let pool_size = mem::size_of_val(&pool)
            + pool
                .chunks
                .iter()
                .map(|chunk| match chunk {
                    StorageType::Continuous(v) => v.capacity(),
                    StorageType::Dedicated(v) => v.capacity(),
                })
                .sum::<usize>();

        println!("Original size: {} bytes", original_size);
        println!("Pool size: {} bytes", pool_size);
        println!(
            "Memory efficiency: {:.2}%",
            (pool_size as f64 / original_size as f64) * 100.0
        );
    }
}
