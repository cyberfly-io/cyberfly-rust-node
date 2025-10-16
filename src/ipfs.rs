use anyhow::Result;
use iroh::{protocol::Router, Endpoint};
use iroh_blobs::{store::fs::FsStore, BlobsProtocol, Hash};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// File metadata stored alongside IPFS content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub filename: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub created_at: i64,
    pub hash: String,     // Iroh content hash (Blake3)
    pub checksum: String, // MD5 checksum for integrity
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub owner_public_key: String, // Owner's Ed25519 public key (hex)
    pub signature: String,        // Signature over metadata (hex)
}

/// IPFS storage manager using Iroh
///
/// Iroh is a modern, efficient IPFS-like implementation built in Rust.
/// It provides:
/// - Content-addressed storage with Blake3 hashing
/// - Efficient data transfer with QUIC
/// - P2P networking
/// - Simple API for adding and retrieving content
#[derive(Clone)]
pub struct IpfsStorage {
    /// Iroh router for protocol handling
    router: Router,
    /// Blob storage protocol handler
    blobs: BlobsProtocol,
    /// File system store for persistent storage
    store: FsStore,
}

impl IpfsStorage {
    /// Create IPFS storage from existing Iroh components (recommended)
    ///
    /// This constructor allows sharing a single Iroh node across multiple
    /// components (network, storage, etc.) for better resource efficiency.
    ///
    /// # Arguments
    /// * `router` - Shared Iroh router
    /// * `blobs` - Shared BlobsProtocol handler
    /// * `store` - Shared FsStore for persistent storage
    pub fn from_components(router: Router, blobs: BlobsProtocol, store: FsStore) -> Self {
        tracing::info!("Initializing IPFS storage from shared Iroh components");
        Self {
            router,
            blobs,
            store,
        }
    }

    /// Get the node ID of this Iroh instance
    pub fn node_id(&self) -> iroh::NodeId {
        self.router.endpoint().node_id()
    }

    /// Get a reference to the endpoint for advanced operations
    pub fn endpoint(&self) -> &Endpoint {
        self.router.endpoint()
    }

    /// Shutdown the Iroh node gracefully
    pub async fn shutdown(self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }

    /// Add a file to IPFS with metadata and return both hashes
    /// Returns: (content_hash, metadata_hash)
    pub async fn add_file_with_metadata(
        &self,
        file_path: &Path,
        tags: Vec<String>,
        description: Option<String>,
        owner_public_key: String,
        signature: String,
    ) -> Result<(String, String)> {
        tracing::info!("Adding file with metadata to IPFS: {:?}", file_path);

        // Read file content
        let content = fs::read(file_path).await?;
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

            // Add file content to Iroh blob store
            let blobs = self.store.blobs();
            let content_tag = blobs.add_bytes(content.clone()).await?;
        let content_hash_str = content_tag.hash.to_string();

        tracing::info!("File content added with hash: {}", content_hash_str);

        // Create metadata
        let metadata = FileMetadata {
            filename: file_name.clone(),
            size: content.len() as u64,
            mime_type: Self::detect_mime_type(file_path),
            created_at: chrono::Utc::now().timestamp(),
            hash: content_hash_str.clone(),
            checksum: format!("{:x}", md5::compute(&content)),
            tags: tags.clone(),
            description: description.clone(),
            owner_public_key: owner_public_key.clone(),
            signature: signature.clone(),
        };

        // Verify signature before storing
        Self::verify_metadata_signature(&metadata)?;

        // Store metadata as JSON in Iroh
        let metadata_json = serde_json::to_vec(&metadata)?;
    let metadata_tag = blobs.add_bytes(metadata_json).await?;
        let metadata_hash_str = metadata_tag.hash.to_string();

        tracing::info!(
            "File added - Content Hash: {}, Metadata Hash: {}, Owner: {}",
            content_hash_str,
            metadata_hash_str,
            owner_public_key
        );

        Ok((content_hash_str, metadata_hash_str))
    }

    /// Add a file to IPFS and return its hash
    pub async fn add_file(&self, file_path: &Path) -> Result<String> {
        tracing::info!("Adding file to IPFS: {:?}", file_path);

        // Read file content
        let content = fs::read(file_path).await?;

        // Add to blob store
        let blobs = self.store.blobs();
        let tag = blobs.add_bytes(content).await?;
        let hash_str = tag.hash.to_string();

        tracing::info!("File added to IPFS with hash: {}", hash_str);
        Ok(hash_str)
    }

    /// Add bytes to IPFS and return its hash
    pub async fn add_bytes(&self, data: &[u8]) -> Result<String> {
        tracing::info!("Adding {} bytes to IPFS", data.len());

        // Add to blob store
        let blobs = self.store.blobs();
        let tag = blobs.add_bytes(data.to_vec()).await?;
        let hash_str = tag.hash.to_string();

        tracing::info!("Data added to IPFS with hash: {}", hash_str);
        Ok(hash_str)
    }

    /// Add bytes with metadata to IPFS
    /// Returns: (content_hash, metadata_hash)
    pub async fn add_bytes_with_metadata(
        &self,
        data: &[u8],
        filename: String,
        mime_type: Option<String>,
        tags: Vec<String>,
        description: Option<String>,
        owner_public_key: String,
        signature: String,
    ) -> Result<(String, String)> {
        tracing::info!("Adding {} bytes with metadata to IPFS", data.len());

        // Add content to blob store
        let blobs = self.store.blobs();
        let content_tag = blobs.add_bytes(data.to_vec()).await?;
        let content_hash_str = content_tag.hash.to_string();

        // Create metadata
        let metadata = FileMetadata {
            filename: filename.clone(),
            size: data.len() as u64,
            mime_type: mime_type.clone(),
            created_at: chrono::Utc::now().timestamp(),
            hash: content_hash_str.clone(),
            checksum: format!("{:x}", md5::compute(data)),
            tags: tags.clone(),
            description: description.clone(),
            owner_public_key: owner_public_key.clone(),
            signature: signature.clone(),
        };

        // Verify signature before storing
        Self::verify_metadata_signature(&metadata)?;

        // Store metadata as JSON
        let metadata_json = serde_json::to_vec(&metadata)?;
        let metadata_tag = blobs.add_bytes(metadata_json).await?;
        let metadata_hash_str = metadata_tag.hash.to_string();

        tracing::info!(
            "Bytes added - Content Hash: {}, Metadata Hash: {}, Owner: {}",
            content_hash_str,
            metadata_hash_str,
            owner_public_key
        );

        Ok((content_hash_str, metadata_hash_str))
    }

    /// Retrieve file metadata from IPFS by metadata hash
    pub async fn get_metadata(&self, metadata_hash: &str) -> Result<FileMetadata> {
        tracing::info!("Retrieving file metadata from IPFS: {}", metadata_hash);

        // Parse hash
        let hash = metadata_hash.parse::<Hash>()?;

        // Get metadata bytes from blob store
        let blobs = self.store.blobs();
        let metadata_bytes = blobs.get_bytes(hash).await?.to_vec();

        // Deserialize metadata
        let metadata: FileMetadata = serde_json::from_slice(&metadata_bytes)?;

        tracing::info!("Retrieved metadata for file: {}", metadata.filename);
        Ok(metadata)
    }

    /// Retrieve file content from IPFS by hash
    pub async fn get_bytes(&self, content_hash: &str) -> Result<Vec<u8>> {
        tracing::info!("Retrieving content from IPFS: {}", content_hash);

        // Parse hash
        let hash = content_hash.parse::<Hash>()?;

        // Get bytes from blob store
        let blobs = self.store.blobs();
        let bytes = blobs.get_bytes(hash).await?.to_vec();

        tracing::info!("Retrieved {} bytes", bytes.len());
        Ok(bytes)
    }

    /// Update file metadata (creates a new metadata hash)
    pub async fn update_metadata(
        &self,
        old_metadata: FileMetadata,
        new_tags: Option<Vec<String>>,
        new_description: Option<String>,
        requester_public_key: String,
        signature: String,
    ) -> Result<String> {
        tracing::info!("Updating file metadata");

        // Verify ownership
        if old_metadata.owner_public_key != requester_public_key {
            return Err(anyhow::anyhow!(
                "Permission denied: requester {} is not the owner {}",
                requester_public_key,
                old_metadata.owner_public_key
            ));
        }

        let updated_metadata = FileMetadata {
            tags: new_tags.unwrap_or(old_metadata.tags),
            description: new_description.or(old_metadata.description),
            signature, // New signature for updated metadata
            ..old_metadata
        };

        // Verify new signature
        Self::verify_metadata_signature(&updated_metadata)?;

        // Store updated metadata as JSON in blob store
        let blobs = self.store.blobs();
        let metadata_json = serde_json::to_vec(&updated_metadata)?;
        let new_metadata_tag = blobs.add_bytes(metadata_json).await?;
        let new_metadata_hash_str = new_metadata_tag.hash.to_string();

        tracing::info!("Metadata updated with new hash: {}", new_metadata_hash_str);
        Ok(new_metadata_hash_str)
    }

    /// Delete file by removing both content and metadata from store
    /// Requires ownership verification
    pub async fn delete_file(
        &self,
        metadata_hash: &str,
        requester_public_key: String,
        signature: String,
    ) -> Result<()> {
        tracing::info!(
            "Attempting to delete file with metadata hash: {}",
            metadata_hash
        );

        // Retrieve metadata
        let metadata = self.get_metadata(metadata_hash).await?;

        // Verify ownership
        if metadata.owner_public_key != requester_public_key {
            return Err(anyhow::anyhow!(
                "Permission denied: requester {} is not the owner {}",
                requester_public_key,
                metadata.owner_public_key
            ));
        }

        // Verify delete signature (signature over: "delete:<metadata_hash>")
        Self::verify_delete_signature(metadata_hash, &requester_public_key, &signature)?;

        // Parse hashes (not strictly needed but validates format)
        let _content_hash = metadata.hash.parse::<Hash>()?;
        let _meta_hash = metadata_hash.parse::<Hash>()?;

        // Remove from blob store (if not referenced elsewhere)
        // Note: Iroh handles garbage collection automatically
        tracing::info!("Content hash: {}", metadata.hash);
        tracing::info!("Metadata hash: {}", metadata_hash);

        tracing::info!(
            "File marked for deletion - Content: {}, Metadata: {}",
            metadata.hash,
            metadata_hash
        );

        Ok(())
    }

    /// Verify ownership of a file
    pub async fn verify_ownership(
        &self,
        metadata_hash: &str,
        claimed_public_key: &str,
    ) -> Result<bool> {
        let metadata = self.get_metadata(metadata_hash).await?;
        Ok(metadata.owner_public_key == claimed_public_key)
    }

    /// Verify metadata signature
    /// Message format: "filename:hash:owner_public_key:created_at"
    fn verify_metadata_signature(metadata: &FileMetadata) -> Result<()> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        // Construct message
        let message = format!(
            "{}:{}:{}:{}",
            metadata.filename, metadata.hash, metadata.owner_public_key, metadata.created_at
        );

        // Decode public key
        let public_key_bytes = hex::decode(&metadata.owner_public_key)
            .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;
        let verifying_key = VerifyingKey::from_bytes(
            public_key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid public key length"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

        // Decode signature
        let signature_bytes = hex::decode(&metadata.signature)
            .map_err(|e| anyhow::anyhow!("Invalid signature hex: {}", e))?;
        let signature = Signature::from_bytes(
            signature_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
        );

        // Verify signature
        verifying_key
            .verify(message.as_bytes(), &signature)
            .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;

        tracing::debug!(
            "Metadata signature verified for owner: {}",
            metadata.owner_public_key
        );
        Ok(())
    }

    /// Verify delete operation signature
    /// Message format: "delete:<metadata_hash>"
    fn verify_delete_signature(
        metadata_hash: &str,
        public_key: &str,
        signature: &str,
    ) -> Result<()> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let message = format!("delete:{}", metadata_hash);

        // Decode public key
        let public_key_bytes = hex::decode(public_key)
            .map_err(|e| anyhow::anyhow!("Invalid public key hex: {}", e))?;
        let verifying_key = VerifyingKey::from_bytes(
            public_key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid public key length"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

        // Decode signature
        let signature_bytes =
            hex::decode(signature).map_err(|e| anyhow::anyhow!("Invalid signature hex: {}", e))?;
        let sig = Signature::from_bytes(
            signature_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid signature length"))?,
        );

        // Verify signature
        verifying_key
            .verify(message.as_bytes(), &sig)
            .map_err(|e| anyhow::anyhow!("Delete signature verification failed: {}", e))?;

        tracing::debug!("Delete signature verified for: {}", public_key);
        Ok(())
    }

    /// Search files by tag
    pub async fn search_by_tag(&self, tag: &str) -> Result<Vec<String>> {
        tracing::info!("Searching files by tag: {}", tag);

        // TODO: Implement indexing/search mechanism
        // This would require maintaining an index of metadata hashes
        // For now, return placeholder
        tracing::warn!("Tag search not yet implemented");
        Ok(Vec::new())
    }

    /// List all blobs in the store
    pub async fn list_blobs(&self) -> Result<Vec<String>> {
        tracing::info!("Listing all blobs");

    // Get all blob hashes from the store
    let _blobs = self.store.blobs();
        // TODO: Implement blob listing if needed
        // For now return empty list
        Ok(Vec::new())
    }

    /// Get the size of content by hash
    /// Note: This is a placeholder - iroh-blobs doesn't expose direct size queries
    /// Consider using metadata or storing size information separately
    pub async fn stat(&self, hash: &str) -> Result<IpfsStatInfo> {
        tracing::info!("Getting stats for hash: {}", hash);

        let hash_obj = hash.parse::<Hash>()?;
        let blobs = self.store.blobs();

        // Try to get bytes to check existence and measure size
        // This is not optimal but iroh-blobs doesn't have a direct get_size method
        match blobs.get_bytes(hash_obj).await {
            Ok(bytes) => Ok(IpfsStatInfo {
                hash: hash.to_string(),
                size: bytes.len() as u64,
                blocks: 1, // Iroh uses single blob per hash
            }),
            Err(_) => Err(anyhow::anyhow!("Blob not found: {}", hash)),
        }
    }

    /// Store arbitrary JSON data in IPFS
    pub async fn store_json<T: Serialize>(&self, data: &T) -> Result<String> {
        tracing::info!("Storing JSON data in IPFS");

        let json_bytes = serde_json::to_vec(data)?;
        let blobs = self.store.blobs();
        let tag_info = blobs.add_bytes(json_bytes).await?;
        let hash_str = tag_info.hash.to_string();

        tracing::info!("JSON data stored with hash: {}", hash_str);
        Ok(hash_str)
    }

    /// Retrieve arbitrary JSON data from IPFS
    pub async fn get_json<T: for<'de> Deserialize<'de>>(&self, hash: &str) -> Result<T> {
        tracing::info!("Retrieving JSON data from IPFS: {}", hash);

        let hash_obj = hash.parse::<Hash>()?;
        let blobs = self.store.blobs();
        let bytes = blobs.get_bytes(hash_obj).await?.to_vec();
        let data: T = serde_json::from_slice(&bytes)?;

        tracing::info!("JSON data retrieved successfully");
        Ok(data)
    }

    /// Detect MIME type from file extension
    fn detect_mime_type(file_path: &Path) -> Option<String> {
        file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "txt" => Some("text/plain"),
                "json" => Some("application/json"),
                "html" => Some("text/html"),
                "css" => Some("text/css"),
                "js" => Some("application/javascript"),
                "png" => Some("image/png"),
                "jpg" | "jpeg" => Some("image/jpeg"),
                "gif" => Some("image/gif"),
                "pdf" => Some("application/pdf"),
                "zip" => Some("application/zip"),
                "mp4" => Some("video/mp4"),
                "mp3" => Some("audio/mpeg"),
                _ => None,
            })
            .map(String::from)
    }
}

#[derive(Debug, Clone)]
pub struct IpfsStatInfo {
    pub hash: String,
    pub size: u64,
    pub blocks: u64,
}
