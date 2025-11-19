//! Performance Benchmarks for Vectorized Query Execution
//!
//! This module provides benchmarks comparing interpreted vs vectorized
//! query execution performance.

use crate::execution::columnar::{ColumnarResult, DataType};
use crate::execution::operators::{VectorizedOperators, VectorizedWhereExecutor, VectorizedCondition, VectorizedValue};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

/// Benchmark vectorized WHERE filtering performance
pub fn bench_vectorized_where_filter(c: &mut Criterion) {
    // Create test data
    let mut result = ColumnarResult::new();
    result.add_column("age".to_string(), DataType::Int64, 10000);

    let age_col = result.get_column_mut("age").unwrap();
    for i in 0..10000 {
        age_col.push((i % 100) as i64).unwrap();
    }
    result.row_count = 10000;

    let executor = VectorizedWhereExecutor::new();

    c.bench_function("vectorized_where_filter_10k", |b| {
        b.iter(|| {
            let condition = VectorizedCondition::Greater {
                column: "age".to_string(),
                value: VectorizedValue::Int64(50),
            };
            let filtered = executor.execute(black_box(&result), black_box(&condition)).unwrap();
            black_box(filtered);
        });
    });
}

/// Benchmark vectorized aggregation performance
pub fn bench_vectorized_aggregation(c: &mut Criterion) {
    let operators = VectorizedOperators::new();

    // Create test column
    let mut column = crate::execution::columnar::Column::with_capacity(DataType::Int64, 10000);
    for i in 0..10000 {
        column.push(i as i64).unwrap();
    }

    c.bench_function("vectorized_sum_10k", |b| {
        b.iter(|| {
            let sum = operators.aggregate_sum_i64(black_box(&column)).unwrap();
            black_box(sum);
        });
    });

    c.bench_function("vectorized_avg_10k", |b| {
        b.iter(|| {
            let avg = operators.aggregate_avg_i64(black_box(&column)).unwrap();
            black_box(avg);
        });
    });
}

/// Benchmark columnar data structure operations
pub fn bench_columnar_operations(c: &mut Criterion) {
    c.bench_function("column_push_10k", |b| {
        b.iter(|| {
            let mut column = crate::execution::columnar::Column::with_capacity(DataType::Int64, 10000);
            for i in 0..10000 {
                column.push(black_box(i as i64)).unwrap();
            }
            black_box(column);
        });
    });

    c.bench_function("columnar_result_creation", |b| {
        b.iter(|| {
            let mut result = ColumnarResult::new();
            result.add_column("id".to_string(), DataType::Int64, 1000);
            result.add_column("age".to_string(), DataType::Int64, 1000);
            result.add_column("score".to_string(), DataType::Float64, 1000);

            for i in 0..1000 {
                result.get_column_mut("id").unwrap().push(i as i64).unwrap();
                result.get_column_mut("age").unwrap().push((i % 100) as i64).unwrap();
                result.get_column_mut("score").unwrap().push((i as f64) * 1.5).unwrap();
            }
            result.row_count = 1000;

            black_box(result);
        });
    });
}

/// Compare interpreted vs vectorized performance
pub fn bench_interpreted_vs_vectorized(c: &mut Criterion) {
    // Create test data
    let mut result = ColumnarResult::new();
    result.add_column("age".to_string(), DataType::Int64, 10000);

    let age_col = result.get_column_mut("age").unwrap();
    for i in 0..10000 {
        age_col.push((i % 100) as i64).unwrap();
    }
    result.row_count = 10000;

    // Interpreted version (simulated)
    fn interpreted_where(result: &ColumnarResult, threshold: i64) -> Vec<bool> {
        let age_col = result.get_column("age").unwrap();
        let mut mask = Vec::with_capacity(result.row_count);

        for i in 0..result.row_count {
            let age = age_col.get::<i64>(i).unwrap();
            mask.push(age > threshold);
        }

        mask
    }

    // Vectorized version
    let executor = VectorizedWhereExecutor::new();

    c.bench_function("interpreted_where_10k", |b| {
        b.iter(|| {
            let mask = interpreted_where(black_box(&result), black_box(50));
            black_box(mask);
        });
    });

    c.bench_function("vectorized_where_10k", |b| {
        b.iter(|| {
            let condition = VectorizedCondition::Greater {
                column: "age".to_string(),
                value: VectorizedValue::Int64(50),
            };
            let filtered = executor.execute(black_box(&result), black_box(&condition)).unwrap();
            black_box(filtered);
        });
    });
}

criterion_group!(
    benches,
    bench_vectorized_where_filter,
    bench_vectorized_aggregation,
    bench_columnar_operations,
    bench_interpreted_vs_vectorized
);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_data_setup() {
        // Test that benchmark data can be created correctly
        let mut result = ColumnarResult::new();
        result.add_column("age".to_string(), DataType::Int64, 100);

        let age_col = result.get_column_mut("age").unwrap();
        for i in 0..100 {
            age_col.push(i as i64).unwrap();
        }
        result.row_count = 100;

        assert_eq!(result.row_count, 100);
        assert!(result.get_column("age").is_some());
    }

    #[test]
    fn test_performance_comparison() {
        // Create test data
        let mut result = ColumnarResult::new();
        result.add_column("age".to_string(), DataType::Int64, 1000);

        let age_col = result.get_column_mut("age").unwrap();
        for i in 0..1000 {
            age_col.push((i % 100) as i64).unwrap();
        }
        result.row_count = 1000;

        // Test interpreted version
        let interpreted_start = std::time::Instant::now();
        let interpreted_mask = (0..1000)
            .map(|i| {
                let age_col = result.get_column("age").unwrap();
                age_col.get::<i64>(i).unwrap() > 50
            })
            .collect::<Vec<_>>();
        let interpreted_time = interpreted_start.elapsed();

        // Test vectorized version
        let executor = VectorizedWhereExecutor::new();
        let vectorized_start = std::time::Instant::now();
        let condition = VectorizedCondition::Greater {
            column: "age".to_string(),
            value: VectorizedValue::Int64(50),
        };
        let vectorized_result = executor.execute(&result, &condition).unwrap();
        let vectorized_time = vectorized_start.elapsed();

        // Both should produce the same number of results
        assert_eq!(vectorized_result.row_count, interpreted_mask.iter().filter(|&&x| x).count());

        // Vectorized should be reasonably fast (though may not be faster in debug mode)
        println!("Interpreted time: {:?}", interpreted_time);
        println!("Vectorized time: {:?}", vectorized_time);
        println!("Vectorized results: {}", vectorized_result.row_count);
    }
}
