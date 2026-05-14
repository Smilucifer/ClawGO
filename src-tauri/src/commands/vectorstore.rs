use crate::models::VectorSearchResult;
use crate::storage::characters;
use arrow_array::{FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures_util::{StreamExt, TryStreamExt};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::connect;
use std::sync::Arc;
use tauri::command;

const TABLE_NAME: &str = "character_memories";

fn lancedb_path(character_id: &str) -> String {
    characters::char_dir(character_id)
        .join("lancedb")
        .to_string_lossy()
        .to_string()
}

fn memory_schema(dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("page_id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim,
            ),
            false,
        ),
    ]))
}

#[command]
pub async fn vector_upsert(
    character_id: String,
    page_id: String,
    vector: Vec<f32>,
) -> Result<(), String> {
    characters::validate_character_id(&character_id)?;
    let db_path = lancedb_path(&character_id);
    std::fs::create_dir_all(&db_path).map_err(|e| e.to_string())?;
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    let dim = vector.len() as i32;
    if dim == 0 {
        return Err("Empty vector".into());
    }
    let schema = memory_schema(dim);

    let table_names = db
        .table_names()
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let table = if table_names.contains(&TABLE_NAME.to_string()) {
        db.open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| e.to_string())?
    } else {
        db.create_empty_table(TABLE_NAME, schema.clone())
            .execute()
            .await
            .map_err(|e| e.to_string())?
    };

    // Delete existing entry for this page_id
    let escaped = page_id.replace('\'', "''");
    let filter = format!("page_id = '{}'", escaped);
    if let Err(e) = table.delete(&filter).await {
        log::warn!("vector_upsert: failed to delete old entry for {}: {}", page_id, e);
    }

    // Build RecordBatch with one row
    let page_ids = StringArray::from(vec![page_id.as_str()]);
    let values = Float32Array::from(vector);
    let list_array = FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim,
        Arc::new(values),
        None,
    );

    let batch =
        RecordBatch::try_new(schema, vec![Arc::new(page_ids), Arc::new(list_array)])
            .map_err(|e| e.to_string())?;

    table.add(batch).execute().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Batch upsert multiple page_id → vector pairs in a single LanceDB write.
/// Idempotent: drops and recreates the table to ensure clean state.
async fn vector_batch_upsert(
    character_id: &str,
    entries: &[(String, Vec<f32>)],
) -> Result<usize, String> {
    if entries.is_empty() {
        return Ok(0);
    }
    characters::validate_character_id(character_id)?;

    let dim = entries[0].1.len() as i32;
    if dim == 0 {
        return Err("Empty vector in batch".into());
    }
    // Validate dimension consistency
    for (i, (_, v)) in entries.iter().enumerate() {
        if v.len() as i32 != dim {
            return Err(format!(
                "Vector dimension mismatch at index {}: expected {} got {}",
                i, dim, v.len()
            ));
        }
    }

    let db_path = lancedb_path(character_id);
    std::fs::create_dir_all(&db_path).map_err(|e| e.to_string())?;
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    // Drop existing table so create_empty_table is idempotent
    let table_names = db.table_names().execute().await.map_err(|e| e.to_string())?;
    if table_names.contains(&TABLE_NAME.to_string()) {
        db.drop_table(TABLE_NAME, &[]).await.map_err(|e| e.to_string())?;
    }

    let schema = memory_schema(dim);

    let table = db
        .create_empty_table(TABLE_NAME, schema.clone())
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let page_ids: Vec<&str> = entries.iter().map(|(id, _)| id.as_str()).collect();
    let page_ids_array = StringArray::from(page_ids);

    let all_values: Vec<f32> = entries
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    let values_array = Float32Array::from(all_values);
    let list_array = FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim,
        Arc::new(values_array),
        None,
    );

    let batch =
        RecordBatch::try_new(schema, vec![Arc::new(page_ids_array), Arc::new(list_array)])
            .map_err(|e| e.to_string())?;

    table.add(batch).execute().await.map_err(|e| e.to_string())?;
    Ok(entries.len())
}

#[command]
pub async fn vector_search(
    character_id: String,
    query_vector: Vec<f32>,
    top_k: u32,
) -> Result<Vec<VectorSearchResult>, String> {
    characters::validate_character_id(&character_id)?;
    let db_path = lancedb_path(&character_id);
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    let table_names = db
        .table_names()
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    if !table_names.contains(&TABLE_NAME.to_string()) {
        return Ok(Vec::new());
    }

    let table = db
        .open_table(TABLE_NAME)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let stream = table
        .vector_search(query_vector)
        .map_err(|e| e.to_string())?
        .limit(top_k as usize)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for batch in batches {
        for i in 0..batch.num_rows() {
            let page_id = batch
                .column_by_name("page_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .map(|a| a.value(i).to_string())
                .unwrap_or_default();
            let distance = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                .map(|a| a.value(i))
                .unwrap_or(0.0);
            let score = 1.0 / (1.0 + distance as f64);
            out.push(VectorSearchResult {
                page_id,
                score,
                memory: None,
            });
        }
    }
    Ok(out)
}

#[command]
pub async fn vector_delete(
    character_id: String,
    page_id: String,
) -> Result<(), String> {
    characters::validate_character_id(&character_id)?;
    let db_path = lancedb_path(&character_id);
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    let table_names = db
        .table_names()
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    if !table_names.contains(&TABLE_NAME.to_string()) {
        return Ok(());
    }

    let table = db
        .open_table(TABLE_NAME)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let escaped = page_id.replace('\'', "''");
    let filter = format!("page_id = '{}'", escaped);
    if let Err(e) = table.delete(&filter).await {
        log::warn!("vector_delete: failed to delete entry for {}: {}", page_id, e);
    }
    Ok(())
}

/// Clear the vector store for a character. Does NOT rebuild — use
/// `rebuild_vector_index` afterwards to restore vector search.
#[command]
pub async fn reset_vector_store(
    character_id: String,
) -> Result<usize, String> {
    characters::validate_character_id(&character_id)?;
    let db_path = lancedb_path(&character_id);
    if let Err(e) = std::fs::remove_dir_all(&db_path) {
        log::warn!("reset_vector_store: failed to clear lancedb for {}: {e}", character_id);
    }

    let log_path = characters::memory_log_path(&character_id);
    let count = std::fs::read_to_string(&log_path)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);

    log::warn!(
        "reset_vector_store: cleared lancedb for {}, {} log entries remain. Run rebuild_vector_index to restore.",
        character_id, count
    );
    Ok(count)
}

/// Rebuild the vector index from the memory log for a character.
/// Reads all log entries, fetches embeddings concurrently (max 8), and batch-upserts.
#[command]
pub async fn rebuild_vector_index(
    character_id: String,
) -> Result<usize, String> {
    characters::validate_character_id(&character_id)?;

    let entries = characters::read_all_memory_log_entries(&character_id)?;
    let total = entries.len();
    if total == 0 {
        return Ok(0);
    }

    // Extract owned (id, content) pairs so the stream doesn't borrow entries
    let items: Vec<(String, String)> = entries
        .into_iter()
        .map(|e| (e.id, e.content))
        .collect();

    let vectors: Vec<(String, Vec<f32>)> = futures_util::stream::iter(items)
        .map(|(id, content)| async move {
            match super::embedding::fetch_embedding(&content).await {
                Ok(v) => Some((id, v)),
                Err(e) => {
                    log::warn!("rebuild_vector_index: embedding failed for {}: {e}", id);
                    None
                }
            }
        })
        .buffer_unordered(8)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect();

    let count = vectors.len();
    if count > 0 {
        vector_batch_upsert(&character_id, &vectors).await?;
    }

    let marker = characters::char_dir(&character_id).join(".rebuild_pending");

    // Only clear the marker if we successfully rebuilt at least one entry.
    // If count is 0 (e.g. embedding API disabled or all fetches failed),
    // leave the marker so the next lazy rebuild attempt will retry.
    if count > 0 {
        let _ = std::fs::remove_file(&marker);
    } else {
        // Ensure marker exists so lazy rebuild retries later
        let _ = std::fs::write(&marker, b"1");
    }

    log::info!(
        "rebuild_vector_index: rebuilt {} of {} entries for {}",
        count, total, character_id
    );
    Ok(count)
}
