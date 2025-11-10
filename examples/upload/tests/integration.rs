//! Integration tests for file upload example

use bytes::Bytes;
use tokio_stream::{iter, StreamExt};
use upload_example::{calculate_checksum, create_chunks, handle_upload, FileChunk, UploadResult, CHUNK_SIZE};
use quill_core::QuillError;

#[tokio::test]
async fn test_upload_small_file() {
    let data = b"Hello, World!";
    let chunks = create_chunks(data, 5);

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].chunk_index, 0);
    assert_eq!(chunks[0].total_chunks, 3);
}

#[tokio::test]
async fn test_upload_with_checksum() {
    let data = b"Test data for upload";
    let expected_checksum = calculate_checksum(data);

    // Create chunks
    let chunks = create_chunks(data, 10);

    // Encode chunks as stream
    let chunk_stream = iter(chunks.into_iter().map(|c| Ok(c.encode())));

    // Handle upload
    let result = handle_upload(Box::pin(chunk_stream)).await.unwrap();

    // Decode result
    let upload_result = UploadResult::decode(&result).unwrap();

    assert_eq!(upload_result.total_bytes, data.len() as u64);
    assert_eq!(upload_result.checksum, expected_checksum);
}

#[tokio::test]
async fn test_upload_large_file() {
    // Create 5MB file
    let data = vec![b'x'; 5 * 1024 * 1024];
    let expected_checksum = calculate_checksum(&data);

    let chunks = create_chunks(&data, CHUNK_SIZE);

    assert!(chunks.len() > 1);

    let chunk_stream = iter(chunks.into_iter().map(|c| Ok(c.encode())));
    let result = handle_upload(Box::pin(chunk_stream)).await.unwrap();

    let upload_result = UploadResult::decode(&result).unwrap();

    assert_eq!(upload_result.total_bytes, data.len() as u64);
    assert_eq!(upload_result.checksum, expected_checksum);
}

#[tokio::test]
async fn test_upload_empty_file() {
    let data = b"";
    let chunks = create_chunks(data, CHUNK_SIZE);

    // Empty file should have no chunks or one empty chunk
    assert!(chunks.len() <= 1);
}

#[tokio::test]
async fn test_chunk_sequence_validation() {
    // Create chunks with wrong sequence
    let chunks = vec![
        FileChunk {
            chunk_index: 0,
            total_chunks: 3,
            data: Bytes::from("chunk0"),
        },
        FileChunk {
            chunk_index: 2, // Skip chunk 1
            total_chunks: 3,
            data: Bytes::from("chunk2"),
        },
    ];

    let chunk_stream = iter(chunks.into_iter().map(|c| Ok(c.encode())));
    let result = handle_upload(Box::pin(chunk_stream)).await;

    // Should error due to sequence mismatch
    assert!(result.is_err());
}

#[tokio::test]
async fn test_upload_exactly_one_chunk() {
    let data = vec![b'a'; 100];
    let chunks = create_chunks(&data, 1000); // Chunk size larger than data

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_index, 0);
    assert_eq!(chunks[0].total_chunks, 1);
}

#[tokio::test]
async fn test_checksum_consistency() {
    let data = b"Consistent data";

    let checksum1 = calculate_checksum(data);
    let checksum2 = calculate_checksum(data);

    assert_eq!(checksum1, checksum2);
}

#[tokio::test]
async fn test_checksum_different_data() {
    let data1 = b"Data version 1";
    let data2 = b"Data version 2";

    let checksum1 = calculate_checksum(data1);
    let checksum2 = calculate_checksum(data2);

    assert_ne!(checksum1, checksum2);
}
