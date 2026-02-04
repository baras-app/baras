use datafusion::arrow::array::{
    Array, Float32Array, Float64Array, Int32Array, Int64Array, LargeStringArray, StringArray,
    StringViewArray, TimestampMillisecondArray, UInt64Array,
};
use datafusion::arrow::record_batch::RecordBatch;

// Re-export query types from shared types crate
// ─────────────────────────────────────────────────────────────────────────────
// Generic Column Extractors (handles Arrow type variations automatically)
// ─────────────────────────────────────────────────────────────────────────────

pub fn col_strings(batch: &RecordBatch, idx: usize) -> Result<Vec<String>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<StringViewArray>() {
        return Ok((0..a.len()).map(|i| a.value(i).to_string()).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<StringArray>() {
        return Ok((0..a.len()).map(|i| a.value(i).to_string()).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<LargeStringArray>() {
        return Ok((0..a.len()).map(|i| a.value(i).to_string()).collect());
    }
    Err(format!(
        "col {idx}: expected string, got {:?}",
        col.data_type()
    ))
}

pub fn col_i64(batch: &RecordBatch, idx: usize) -> Result<Vec<i64>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<Int64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Int32Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as i64).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<UInt64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as i64).collect());
    }
    Err(format!(
        "col {idx}: expected int, got {:?}",
        col.data_type()
    ))
}

pub fn col_f64(batch: &RecordBatch, idx: usize) -> Result<Vec<f64>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<Float64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Float32Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as f64).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Int64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as f64).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Int32Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as f64).collect());
    }
    Err(format!(
        "col {idx}: expected float, got {:?}",
        col.data_type()
    ))
}

pub fn col_f32(batch: &RecordBatch, idx: usize) -> Result<Vec<f32>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<Float32Array>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Float64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as f32).collect());
    }
    Err(format!(
        "col {idx}: expected float, got {:?}",
        col.data_type()
    ))
}

pub fn scalar_f32(batches: &[RecordBatch]) -> f32 {
    batches
        .first()
        .and_then(|b| {
            if b.num_rows() == 0 {
                return None;
            }
            col_f32(b, 0).ok().and_then(|v| v.first().copied())
        })
        .unwrap_or(0.0)
}

pub fn col_i32(batch: &RecordBatch, idx: usize) -> Result<Vec<i32>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<Int32Array>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    if let Some(a) = col.as_any().downcast_ref::<Int64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i) as i32).collect());
    }
    Err(format!(
        "col {idx}: expected i32, got {:?}",
        col.data_type()
    ))
}

pub fn col_bool(batch: &RecordBatch, idx: usize) -> Result<Vec<bool>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<arrow::array::BooleanArray>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    Err(format!(
        "col {idx}: expected bool, got {:?}",
        col.data_type()
    ))
}

pub fn col_timestamp_ms(batch: &RecordBatch, idx: usize) -> Result<Vec<i64>, String> {
    let col = batch.column(idx);
    if let Some(a) = col.as_any().downcast_ref::<TimestampMillisecondArray>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    // Fall back to regular i64 if stored differently
    if let Some(a) = col.as_any().downcast_ref::<Int64Array>() {
        return Ok((0..a.len()).map(|i| a.value(i)).collect());
    }
    Err(format!(
        "col {idx}: expected timestamp, got {:?}",
        col.data_type()
    ))
}
