//! Query Codec for Request-Response Serialization
//!
//! Implements the libp2p Codec trait for Query/QueryResponse serialization
//! using length-prefixed JSON format.
//!
//! Implements Requirements 1.5, 9.1, 9.2, 9.3, 9.5

use crate::protocol::queries::{Query, QueryResponse, MAX_QUERY_SIZE, QUERY_PROTOCOL_ID};
use async_trait::async_trait;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::request_response::Codec;
use libp2p::StreamProtocol;
use std::io;
use tracing::{debug, warn};

/// Codec for Query/QueryResponse serialization
/// 
/// Uses length-prefixed JSON format:
/// - 4 bytes: length (big-endian u32)
/// - N bytes: JSON data
#[derive(Debug, Clone, Default)]
pub struct QueryCodec;

impl QueryCodec {
    /// Create a new QueryCodec
    pub fn new() -> Self {
        Self
    }
    
    /// Get the protocol as StreamProtocol
    pub fn protocol() -> StreamProtocol {
        StreamProtocol::new(QUERY_PROTOCOL_ID)
    }
}

#[async_trait]
impl Codec for QueryCodec {
    type Protocol = StreamProtocol;
    type Request = Query;
    type Response = QueryResponse;
    
    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes, big-endian)
        let mut length_bytes = [0u8; 4];
        io.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        
        // Enforce max size (1MB) - Property 17
        if length > MAX_QUERY_SIZE {
            warn!("Query too large: {} bytes (max {})", length, MAX_QUERY_SIZE);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Query too large: {} bytes (max {} bytes)", length, MAX_QUERY_SIZE),
            ));
        }
        
        // Read JSON data
        let mut buffer = vec![0u8; length];
        io.read_exact(&mut buffer).await?;
        
        // Deserialize - Property 16
        serde_json::from_slice(&buffer).map_err(|e| {
            warn!("Failed to deserialize query: {}", e);
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Deserialization error: {}", e),
            )
        })
    }
    
    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes, big-endian)
        let mut length_bytes = [0u8; 4];
        io.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        
        // Enforce max size (1MB)
        if length > MAX_QUERY_SIZE {
            warn!("Response too large: {} bytes (max {})", length, MAX_QUERY_SIZE);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Response too large: {} bytes (max {} bytes)", length, MAX_QUERY_SIZE),
            ));
        }
        
        // Read JSON data
        let mut buffer = vec![0u8; length];
        io.read_exact(&mut buffer).await?;
        
        // Deserialize
        serde_json::from_slice(&buffer).map_err(|e| {
            warn!("Failed to deserialize response: {}", e);
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Deserialization error: {}", e),
            )
        })
    }
    
    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Serialize - Property 15
        let data = serde_json::to_vec(&req).map_err(|e| {
            warn!("Failed to serialize query: {}", e);
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Serialization error: {}", e),
            )
        })?;
        
        // Check size limit
        if data.len() > MAX_QUERY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Query too large: {} bytes (max {} bytes)", data.len(), MAX_QUERY_SIZE),
            ));
        }
        
        // Write length prefix
        let length = (data.len() as u32).to_be_bytes();
        io.write_all(&length).await?;
        
        // Write JSON data
        io.write_all(&data).await?;
        io.flush().await?;
        
        debug!("Wrote query: {} bytes", data.len());
        Ok(())
    }
    
    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Serialize
        let data = serde_json::to_vec(&res).map_err(|e| {
            warn!("Failed to serialize response: {}", e);
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Serialization error: {}", e),
            )
        })?;
        
        // Check size limit
        if data.len() > MAX_QUERY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Response too large: {} bytes (max {} bytes)", data.len(), MAX_QUERY_SIZE),
            ));
        }
        
        // Write length prefix
        let length = (data.len() as u32).to_be_bytes();
        io.write_all(&length).await?;
        
        // Write JSON data
        io.write_all(&data).await?;
        io.flush().await?;
        
        debug!("Wrote response: {} bytes", data.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::Cursor;
    
    #[tokio::test]
    async fn test_query_roundtrip() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        // Test query
        let query = Query::get_node_info("test-node");
        
        // Write to buffer
        let mut buffer = Vec::new();
        codec.write_request(&protocol, &mut buffer, query.clone()).await.unwrap();
        
        // Read back
        let mut cursor = Cursor::new(buffer);
        let read_query = codec.read_request(&protocol, &mut cursor).await.unwrap();
        
        assert_eq!(query, read_query);
    }
    
    #[tokio::test]
    async fn test_response_roundtrip() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        // Test response
        let response = QueryResponse::pong();
        
        // Write to buffer
        let mut buffer = Vec::new();
        codec.write_response(&protocol, &mut buffer, response.clone()).await.unwrap();
        
        // Read back
        let mut cursor = Cursor::new(buffer);
        let read_response = codec.read_response(&protocol, &mut cursor).await.unwrap();
        
        assert_eq!(response, read_response);
    }
    
    #[tokio::test]
    async fn test_error_response_roundtrip() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        let response = QueryResponse::error("Test error", "TEST_CODE");
        
        let mut buffer = Vec::new();
        codec.write_response(&protocol, &mut buffer, response.clone()).await.unwrap();
        
        let mut cursor = Cursor::new(buffer);
        let read_response = codec.read_response(&protocol, &mut cursor).await.unwrap();
        
        assert_eq!(response, read_response);
    }
    
    #[tokio::test]
    async fn test_size_limit_enforcement() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        // Create a buffer with size exceeding limit
        let oversized_length = (MAX_QUERY_SIZE + 1) as u32;
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&oversized_length.to_be_bytes());
        buffer.extend(vec![0u8; 100]); // Just some data
        
        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("too large"));
    }
    
    #[tokio::test]
    async fn test_deserialization_error() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        // Create invalid JSON data
        let invalid_json = b"not valid json";
        let length = (invalid_json.len() as u32).to_be_bytes();
        
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&length);
        buffer.extend_from_slice(invalid_json);
        
        let mut cursor = Cursor::new(buffer);
        let result = codec.read_request(&protocol, &mut cursor).await;
        
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Deserialization error"));
    }
    
    #[tokio::test]
    async fn test_all_query_types_roundtrip() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        let queries = vec![
            Query::get_node_info("node-1"),
            Query::get_node_metrics("node-1"),
            Query::get_node_health("node-1"),
            Query::list_nodes(),
            Query::get_job_status("job-1"),
            Query::list_jobs(None),
            Query::get_job_result("job-1"),
            Query::ping(),
        ];
        
        for query in queries {
            let mut buffer = Vec::new();
            codec.write_request(&protocol, &mut buffer, query.clone()).await.unwrap();
            
            let mut cursor = Cursor::new(buffer);
            let read_query = codec.read_request(&protocol, &mut cursor).await.unwrap();
            
            assert_eq!(query, read_query);
        }
    }
    
    #[tokio::test]
    async fn test_length_prefix_format() {
        let mut codec = QueryCodec::new();
        let protocol = QueryCodec::protocol();
        
        let query = Query::ping();
        
        let mut buffer = Vec::new();
        codec.write_request(&protocol, &mut buffer, query).await.unwrap();
        
        // First 4 bytes should be length
        let length_bytes: [u8; 4] = buffer[..4].try_into().unwrap();
        let length = u32::from_be_bytes(length_bytes) as usize;
        
        // Length should match remaining buffer size
        assert_eq!(length, buffer.len() - 4);
    }
}



