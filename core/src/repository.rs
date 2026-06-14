use crate::index::{ChunkLocation, Index, PackInfo};
use crate::pack::{PackFile, PackManager, RepackStats, Repacker};
use crate::snapshot::{Snapshot, Tree};
use crate::storage::{RepositoryLocation, RepositoryStorage, S3Location, storage_for_location};
use crate::{ChunkID, PackID, SnapshotID};
use crate::{
    AzureRepoTransport, Error, RcloneRepoTransport, RepoConfig, RepoTransport, Result, S3RepoSse,
    S3RepoTransport, SftpRepoTransport, crypto::{Encryptor, MasterKey},
};
use bytes::Bytes;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

/// Default pack cache size in bytes (128 MB).
const DEFAULT_PACK_CACHE_SIZE: usize = 128 * 1024 * 1024;

/// Maximum number of packs to cache.
const DEFAULT_PACK_CACHE_COUNT: usize = 32;

/// The main repository structure for Ghostsnap backups.
///
/// A repository manages all backup data including snapshots, pack files, indices, and encryption keys.
/// It provides thread-safe access to backup operations through asynchronous methods.
///
/// # Repository Structure
///
/// ```text
/// repository/
/// ├── config          # Repository configuration
/// ├── keys/           # Encrypted data keys
/// ├── data/           # Pack files and tree objects
/// ├── index/          # Chunk location index (consolidated)
/// │   └── main.idx    # Encrypted binary index file
/// ├── snapshots/      # Snapshot metadata
/// └── locks/          # Repository locks
/// ```
pub struct Repository {
    location: RepositoryLocation,
    display_path: PathBuf,
    storage: Box<dyn RepositoryStorage>,
    config: RepoConfig,
    #[allow(dead_code)]
    master_key: Option<MasterKey>,
    encryptor: Option<Encryptor>,
    /// In-memory chunk index with bloom filter
    index: Arc<RwLock<Index>>,
    /// LRU cache for pack files
    pack_cache: Arc<RwLock<LruCache<PackID, Arc<PackFile>>>>,
    /// Current total size of cached packs
    pack_cache_size: Arc<RwLock<usize>>,
    /// Maximum cache size in bytes
    max_cache_size: usize,
}

impl Repository {
    /// Initializes a new repository at the given path.
    ///
    /// This creates the repository directory structure, generates encryption keys,
    /// and stores the encrypted configuration. The repository is immediately ready for use.
    pub async fn init<P: AsRef<Path>>(path: P, password: &str) -> Result<Self> {
        Self::init_at_location(
            RepositoryLocation::Local(path.as_ref().to_path_buf()),
            password,
        )
        .await
    }

    pub async fn init_at_location(location: RepositoryLocation, password: &str) -> Result<Self> {
        let storage = storage_for_location(&location).await?;

        if storage.exists("config").await? {
            return Err(Error::RepositoryExists {
                path: location.display(),
            });
        }

        storage.init().await?;

        let config = RepoConfig {
            transport: Some(Self::transport_from_location(&location)),
            ..RepoConfig::default()
        };

        let master_key =
            MasterKey::derive_from_password(password, &config.kdf_params.salt, &config.kdf_params)?;

        let data_key = MasterKey::generate();
        let encryptor = Encryptor::new(data_key.as_bytes())?;

        let key_encryptor = Encryptor::new(master_key.as_bytes())?;
        let encrypted_data_key = key_encryptor.encrypt(data_key.as_bytes())?;

        let key_file = KeyFile {
            encrypted_key: encrypted_data_key,
            kdf_params: config.kdf_params.clone(),
        };

        let config_json = serde_json::to_string_pretty(&config)?;
        storage.write("config", Bytes::from(config_json)).await?;

        let key_json = serde_json::to_string_pretty(&key_file)?;
        let key_id = uuid::Uuid::new_v4().to_string();
        storage
            .write(&format!("keys/{}", key_id), Bytes::from(key_json))
            .await?;

        // Create empty index
        let index = Index::new();

        let display_path = PathBuf::from(location.display());
        Ok(Self {
            location,
            display_path,
            storage,
            config,
            master_key: Some(master_key),
            encryptor: Some(encryptor),
            index: Arc::new(RwLock::new(index)),
            pack_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_PACK_CACHE_COUNT).unwrap(),
            ))),
            pack_cache_size: Arc::new(RwLock::new(0)),
            max_cache_size: DEFAULT_PACK_CACHE_SIZE,
        })
    }

    /// Opens an existing repository.
    ///
    /// Loads the repository configuration, decrypts the data keys, and loads the chunk index.
    /// If the repository uses the legacy per-file index format, it will be automatically
    /// migrated to the consolidated format on first access.
    pub async fn open<P: AsRef<Path>>(path: P, password: &str) -> Result<Self> {
        Self::open_at_location(
            RepositoryLocation::Local(path.as_ref().to_path_buf()),
            password,
        )
        .await
    }

    pub async fn open_at_location(location: RepositoryLocation, password: &str) -> Result<Self> {
        let bootstrap_storage = storage_for_location(&location).await?;

        if !bootstrap_storage.exists("config").await? {
            return Err(Error::RepositoryNotFound {
                path: location.display(),
            });
        }

        let config_bytes = bootstrap_storage.read("config").await?;
        let config_data = str::from_utf8(&config_bytes)
            .map_err(|e| Error::Other(format!("Invalid repository config encoding: {}", e)))?;
        let config: RepoConfig = serde_json::from_str(config_data)?;

        if config.version != 1 {
            return Err(Error::InvalidFormatVersion {
                version: config.version,
            });
        }

        let resolved_location = Self::resolve_location(location, &config);
        let storage = storage_for_location(&resolved_location).await?;

        let mut key_file = None;

        for key_name in storage.list("keys").await? {
            let key_data = storage.read(&format!("keys/{}", key_name)).await?;
            let key_data = str::from_utf8(&key_data)
                .map_err(|e| Error::Other(format!("Invalid key file encoding: {}", e)))?;
            if let Ok(kf) = serde_json::from_str::<KeyFile>(key_data) {
                key_file = Some(kf);
                break;
            }
        }

        let key_file = key_file.ok_or(Error::InvalidPassword)?;

        let master_key = MasterKey::derive_from_password(
            password,
            &key_file.kdf_params.salt,
            &key_file.kdf_params,
        )?;

        let key_encryptor = Encryptor::new(master_key.as_bytes())?;
        let data_key = key_encryptor
            .decrypt(&key_file.encrypted_key)
            .map_err(|_| Error::InvalidPassword)?;

        let encryptor = Encryptor::new(&data_key)?;

        // Load index (with migration from legacy format if needed)
        let local_path = match &resolved_location {
            RepositoryLocation::Local(path) => Some(path.clone()),
            RepositoryLocation::S3(_) => None,
            RepositoryLocation::Azure(_) => None,
            RepositoryLocation::Rclone(_) => None,
            RepositoryLocation::Sftp(_) => None,
        };
        let index =
            Self::load_or_migrate_index(storage.as_ref(), local_path.as_deref(), &encryptor)
                .await?;
        let display_path = PathBuf::from(resolved_location.display());

        Ok(Self {
            location: resolved_location,
            display_path,
            storage,
            config,
            master_key: Some(master_key),
            encryptor: Some(encryptor),
            index: Arc::new(RwLock::new(index)),
            pack_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_PACK_CACHE_COUNT).unwrap(),
            ))),
            pack_cache_size: Arc::new(RwLock::new(0)),
            max_cache_size: DEFAULT_PACK_CACHE_SIZE,
        })
    }

    /// Loads the consolidated index or migrates from legacy format.
    async fn load_or_migrate_index(
        storage: &dyn RepositoryStorage,
        local_path: Option<&Path>,
        encryptor: &Encryptor,
    ) -> Result<Index> {
        if storage.exists("index/main.idx").await? {
            let data = storage.read("index/main.idx").await?;
            Index::from_encrypted_bytes(&data, encryptor)
        } else if let Some(local_path) = local_path {
            let index_dir = local_path.join("index");
            let mut has_legacy = false;
            if let Ok(mut entries) = fs::read_dir(&index_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name();
                    if name.to_string_lossy().len() == 64 {
                        has_legacy = true;
                        break;
                    }
                }
            }

            if has_legacy {
                tracing::info!("Migrating legacy index to consolidated format...");
                let index = Index::load_from_legacy_dir(&index_dir).await?;
                let encrypted = index.to_encrypted_bytes(encryptor)?;
                storage.write("index/main.idx", encrypted.into()).await?;
                Self::cleanup_legacy_index(&index_dir).await;
                tracing::info!("Index migration complete: {} chunks", index.chunk_count());
                Ok(index)
            } else {
                Ok(Index::new())
            }
        } else {
            Ok(Index::new())
        }
    }

    /// Removes legacy per-file index entries after migration.
    async fn cleanup_legacy_index(index_dir: &Path) {
        if let Ok(mut entries) = fs::read_dir(index_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Only delete 64-char hex files (chunk IDs), keep .idx files
                if name_str.len() == 64 && !name_str.contains('.') {
                    let _ = fs::remove_file(entry.path()).await;
                }
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.display_path
    }

    pub fn location(&self) -> &RepositoryLocation {
        &self.location
    }

    /// Returns the local filesystem path if this is a local repository.
    /// Returns None for remote repositories (S3, Azure, Rclone, etc.) where file-based locking is not applicable.
    pub fn local_path(&self) -> Option<&Path> {
        match &self.location {
            RepositoryLocation::Local(path) => Some(path),
            RepositoryLocation::S3(_) => None,
            RepositoryLocation::Azure(_) => None,
            RepositoryLocation::Rclone(_) => None,
            RepositoryLocation::Sftp(_) => None,
        }
    }

    fn transport_from_location(location: &RepositoryLocation) -> RepoTransport {
        match location {
            RepositoryLocation::Local(_) => RepoTransport::Local,
            RepositoryLocation::S3(s3) => RepoTransport::S3(S3RepoTransport {
                bucket: s3.bucket.clone(),
                prefix: s3.prefix.clone(),
                endpoint: s3.endpoint.clone(),
                region: s3.region.clone(),
                sse: s3.sse.clone(),
            }),
            RepositoryLocation::Azure(azure) => RepoTransport::Azure(AzureRepoTransport {
                account_name: azure.account_name.clone(),
                container: azure.container.clone(),
                prefix: azure.prefix.clone(),
            }),
            RepositoryLocation::Rclone(rclone) => RepoTransport::Rclone(RcloneRepoTransport {
                remote: rclone.remote.clone(),
                path: rclone.path.clone(),
            }),
            RepositoryLocation::Sftp(sftp) => RepoTransport::Sftp(SftpRepoTransport {
                host: sftp.host.clone(),
                port: sftp.port,
                user: sftp.user.clone(),
                path: sftp.path.clone(),
            }),
        }
    }

    fn resolve_location(input: RepositoryLocation, config: &RepoConfig) -> RepositoryLocation {
        match (input, config.transport.as_ref()) {
            (RepositoryLocation::Local(path), _) => RepositoryLocation::Local(path),
            (RepositoryLocation::S3(mut location), Some(RepoTransport::S3(stored))) => {
                if location.bucket.is_empty() {
                    location.bucket = stored.bucket.clone();
                }
                if location.prefix.is_empty() {
                    location.prefix = stored.prefix.clone();
                }
                if location.endpoint.is_none() {
                    location.endpoint = stored.endpoint.clone();
                }
                if location.region.is_none() {
                    location.region = stored.region.clone();
                }
                // Apply SSE configuration from stored transport
                if location.sse.is_none() {
                    location.sse = stored.sse.clone();
                }
                RepositoryLocation::S3(location)
            }
            (RepositoryLocation::S3(location), _) => RepositoryLocation::S3(location),
            (RepositoryLocation::Azure(mut location), Some(RepoTransport::Azure(stored))) => {
                if location.account_name.is_empty() {
                    location.account_name = stored.account_name.clone();
                }
                if location.container.is_empty() {
                    location.container = stored.container.clone();
                }
                if location.prefix.is_empty() {
                    location.prefix = stored.prefix.clone();
                }
                RepositoryLocation::Azure(location)
            }
            (RepositoryLocation::Azure(location), _) => RepositoryLocation::Azure(location),
            (RepositoryLocation::Rclone(mut location), Some(RepoTransport::Rclone(stored))) => {
                if location.remote.is_empty() {
                    location.remote = stored.remote.clone();
                }
                if location.path.is_empty() {
                    location.path = stored.path.clone();
                }
                RepositoryLocation::Rclone(location)
            }
            (RepositoryLocation::Rclone(location), _) => RepositoryLocation::Rclone(location),
            (RepositoryLocation::Sftp(mut location), Some(RepoTransport::Sftp(stored))) => {
                if location.host.is_empty() {
                    location.host = stored.host.clone();
                }
                if location.user.is_empty() {
                    location.user = stored.user.clone();
                }
                if location.path.is_empty() {
                    location.path = stored.path.clone();
                }
                RepositoryLocation::Sftp(location)
            }
            (RepositoryLocation::Sftp(location), _) => RepositoryLocation::Sftp(location),
        }
    }

    pub fn s3_transport(&self) -> Option<&S3RepoTransport> {
        match self.config.transport.as_ref() {
            Some(RepoTransport::S3(config)) => Some(config),
            _ => None,
        }
    }

    pub async fn set_s3_transport_config(
        &mut self,
        location: &S3Location,
        sse: Option<S3RepoSse>,
    ) -> Result<()> {
        self.config.transport = Some(RepoTransport::S3(S3RepoTransport {
            bucket: location.bucket.clone(),
            prefix: location.prefix.clone(),
            endpoint: location.endpoint.clone(),
            region: location.region.clone(),
            sse,
        }));

        let config_json = serde_json::to_string_pretty(&self.config)?;
        self.storage
            .write("config", Bytes::from(config_json))
            .await?;
        Ok(())
    }

    pub async fn object_size(&self, path: &str) -> Result<u64> {
        Ok(self.storage.metadata(path).await?.size)
    }

    pub async fn pack_size(&self, pack_id: &PackID) -> Result<u64> {
        self.object_size(&format!("data/{}.pack", pack_id)).await
    }

    pub async fn pack_exists(&self, pack_id: &PackID) -> Result<bool> {
        self.storage.exists(&format!("data/{}.pack", pack_id)).await
    }

    pub fn config(&self) -> &RepoConfig {
        &self.config
    }

    pub fn encryptor(&self) -> Result<&Encryptor> {
        self.encryptor
            .as_ref()
            .ok_or_else(|| Error::Other("Repository not unlocked".to_string()))
    }

    /// Returns a clone of the index Arc for shared access.
    pub fn index(&self) -> Arc<RwLock<Index>> {
        Arc::clone(&self.index)
    }

    /// Saves the index if it has unsaved changes.
    pub async fn save_index(&self) -> Result<()> {
        let encryptor = self.encryptor()?;
        let mut index = self.index.write().await;

        if index.is_dirty() {
            let encrypted = index.to_encrypted_bytes(encryptor)?;
            self.storage
                .write("index/main.idx", encrypted.into())
                .await?;
            index.mark_clean();
        }

        Ok(())
    }

    /// Forces an index save regardless of dirty state.
    pub async fn flush_index(&self) -> Result<()> {
        let encryptor = self.encryptor()?;
        let mut index = self.index.write().await;
        let encrypted = index.to_encrypted_bytes(encryptor)?;
        self.storage
            .write("index/main.idx", encrypted.into())
            .await?;
        index.mark_clean();
        Ok(())
    }

    pub async fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let encryptor = self.encryptor()?;
        let data = snapshot.serialize(encryptor)?;
        self.storage
            .write(&format!("snapshots/{}", snapshot.id), data)
            .await?;
        Ok(())
    }

    pub async fn load_snapshot(&self, snapshot_id: &SnapshotID) -> Result<Snapshot> {
        let encryptor = self.encryptor()?;
        let data = self
            .storage
            .read(&format!("snapshots/{}", snapshot_id))
            .await?;
        Snapshot::deserialize(&data, encryptor)
    }

    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotID>> {
        let mut snapshot_ids = self.storage.list("snapshots").await?;
        snapshot_ids.sort();
        Ok(snapshot_ids)
    }

    /// Deletes a snapshot by ID.
    pub async fn delete_snapshot(&self, snapshot_id: &SnapshotID) -> Result<()> {
        self.storage
            .delete(&format!("snapshots/{}", snapshot_id))
            .await?;
        Ok(())
    }

    pub async fn save_tree(&self, tree: &Tree) -> Result<ChunkID> {
        let encryptor = self.encryptor()?;
        let data = tree.serialize(encryptor)?;
        let tree_id = ChunkID::from_data(&data);
        self.storage
            .write(&format!("data/{}", tree_id.to_hex()), data)
            .await?;
        Ok(tree_id)
    }

    pub async fn load_tree(&self, tree_id: &ChunkID) -> Result<Tree> {
        let encryptor = self.encryptor()?;
        let data = self
            .storage
            .read(&format!("data/{}", tree_id.to_hex()))
            .await?;
        Tree::deserialize(&data, encryptor)
    }

    pub async fn save_pack(&self, pack: &PackFile) -> Result<()> {
        let encryptor = self.encryptor()?;
        let bytes = pack.to_encrypted_bytes(encryptor)?;
        self.storage
            .write(&format!("data/{}.pack", pack.header.pack_id), bytes.into())
            .await?;

        // Invalidate cache entry if it exists
        {
            let mut cache = self.pack_cache.write().await;
            let mut cache_size = self.pack_cache_size.write().await;
            if let Some(old_pack) = cache.pop(&pack.header.pack_id) {
                *cache_size = cache_size.saturating_sub(old_pack.size());
            }
        }

        // Update index with pack info
        let mut index = self.index.write().await;
        index.add_pack(PackInfo {
            id: pack.header.pack_id.clone(),
            size: pack.header.compressed_size,
            chunk_count: pack.header.chunk_count,
        });

        Ok(())
    }

    /// Loads a pack file, using the LRU cache if available.
    pub async fn load_pack(&self, pack_id: &PackID) -> Result<Arc<PackFile>> {
        // Check cache first
        {
            let mut cache = self.pack_cache.write().await;
            if let Some(pack) = cache.get(pack_id) {
                tracing::debug!("Pack cache hit: {}", pack_id);
                return Ok(Arc::clone(pack));
            }
        }

        // Cache miss - load from disk
        tracing::debug!("Pack cache miss: {}", pack_id);
        let encryptor = self.encryptor()?;
        let data = self.storage.read(&format!("data/{}.pack", pack_id)).await?;
        let pack = PackFile::from_encrypted_bytes(&data, encryptor)?;
        let pack_size = pack.size();
        let pack = Arc::new(pack);

        // Add to cache with LRU eviction
        {
            let mut cache = self.pack_cache.write().await;
            let mut cache_size = self.pack_cache_size.write().await;

            // Evict oldest entries if over size limit
            while *cache_size + pack_size > self.max_cache_size && !cache.is_empty() {
                if let Some((evicted_id, evicted_pack)) = cache.pop_lru() {
                    *cache_size = cache_size.saturating_sub(evicted_pack.size());
                    tracing::debug!("Evicted pack {} from cache", evicted_id);
                }
            }

            cache.put(pack_id.clone(), Arc::clone(&pack));
            *cache_size += pack_size;
        }

        Ok(pack)
    }

    /// Lists all pack files in the repository.
    pub async fn list_packs(&self) -> Result<Vec<PackID>> {
        let entries = self.storage.list("data").await?;
        let mut pack_ids = Vec::new();

        for name in entries {
            if name.ends_with(".pack") {
                pack_ids.push(name.trim_end_matches(".pack").to_string());
            }
        }

        Ok(pack_ids)
    }

    /// Deletes a pack file.
    pub async fn delete_pack(&self, pack_id: &PackID) -> Result<()> {
        // Invalidate cache entry
        {
            let mut cache = self.pack_cache.write().await;
            let mut cache_size = self.pack_cache_size.write().await;
            if let Some(old_pack) = cache.pop(pack_id) {
                *cache_size = cache_size.saturating_sub(old_pack.size());
            }
        }

        self.storage
            .delete(&format!("data/{}.pack", pack_id))
            .await?;

        // Remove from index
        let mut index = self.index.write().await;
        index.remove_pack(pack_id);

        Ok(())
    }

    /// Checks if a chunk exists using the in-memory index with bloom filter.
    /// This is O(1) for chunks that don't exist (bloom filter) and O(1) amortized
    /// for chunks that do exist (HashMap lookup).
    pub async fn has_chunk(&self, chunk_id: &ChunkID) -> Result<bool> {
        let index = self.index.read().await;
        Ok(index.has_chunk(chunk_id))
    }

    /// Adds a chunk location to the index.
    pub async fn save_chunk_location(
        &self,
        chunk_id: &ChunkID,
        pack_id: &PackID,
        offset: u64,
        length: u32,
    ) -> Result<()> {
        let mut index = self.index.write().await;
        index.add_chunk(
            *chunk_id,
            ChunkLocation {
                pack_id: pack_id.clone(),
                offset,
                length,
            },
        );
        Ok(())
    }

    /// Gets chunk location from the in-memory index.
    pub async fn load_chunk_location(&self, chunk_id: &ChunkID) -> Result<ChunkLocation> {
        let index = self.index.read().await;
        index
            .get_chunk(chunk_id)
            .cloned()
            .ok_or_else(|| Error::ChunkNotFound {
                id: chunk_id.to_hex(),
            })
    }

    /// Loads a chunk's data by looking up its location and reading from the pack.
    pub async fn load_chunk(&self, chunk_id: &ChunkID) -> Result<Bytes> {
        let location = self.load_chunk_location(chunk_id).await?;
        let pack = self.load_pack(&location.pack_id).await?;
        pack.get_chunk(chunk_id)
    }

    /// Returns repository statistics.
    pub async fn stats(&self) -> RepoStats {
        let index = self.index.read().await;
        RepoStats {
            chunk_count: index.chunk_count(),
            pack_count: index.pack_count(),
        }
    }

    /// Returns pack cache statistics.
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.pack_cache.read().await;
        let cache_size = self.pack_cache_size.read().await;

        CacheStats {
            pack_count: cache.len(),
            total_size: *cache_size,
            max_size: self.max_cache_size,
        }
    }

    /// Collects all chunk IDs referenced by all snapshots in the repository.
    pub async fn collect_used_chunks(&self) -> Result<std::collections::HashSet<ChunkID>> {
        use std::collections::HashSet;

        let mut used_chunks = HashSet::new();
        let snapshot_ids = self.list_snapshots().await?;

        for snapshot_id in snapshot_ids {
            let snapshot = self.load_snapshot(&snapshot_id).await?;
            let tree = self.load_tree(&snapshot.tree).await?;

            for node in &tree.nodes {
                for chunk_ref in &node.chunks {
                    used_chunks.insert(chunk_ref.id);
                }
            }
        }

        Ok(used_chunks)
    }

    /// Compacts the index by removing unreferenced chunks.
    /// Returns the number of chunks removed.
    pub async fn compact_index(&self) -> Result<usize> {
        let used_chunks = self.collect_used_chunks().await?;
        let mut index = self.index.write().await;
        let removed = index.compact(&used_chunks);
        Ok(removed)
    }

    /// Identifies packs that contain no referenced chunks.
    pub async fn find_unused_packs(&self) -> Result<Vec<PackID>> {
        let used_chunks = self.collect_used_chunks().await?;
        let index = self.index.read().await;

        let mut unused_packs = Vec::new();

        for pack_id in index.all_pack_ids() {
            let pack_chunks = index.chunks_in_pack(&pack_id);
            let has_used_chunks = pack_chunks.iter().any(|id| used_chunks.contains(id));

            if !has_used_chunks {
                unused_packs.push(pack_id);
            }
        }

        Ok(unused_packs)
    }

    /// Prunes unused packs from the repository.
    /// Returns statistics about what was removed.
    pub async fn prune_packs(&self) -> Result<CompactStats> {
        let unused_packs = self.find_unused_packs().await?;
        let mut bytes_freed = 0u64;

        for pack_id in &unused_packs {
            // Get pack info for size
            {
                let index = self.index.read().await;
                if let Some(info) = index.get_pack(pack_id) {
                    bytes_freed += info.size;
                }
            }

            // Delete the pack file and update index
            self.delete_pack(pack_id).await?;
        }

        // Compact index to remove orphaned chunks
        let chunks_removed = self.compact_index().await?;

        Ok(CompactStats {
            chunks_removed,
            packs_removed: unused_packs.len(),
            bytes_freed,
        })
    }

    /// Repacks the repository by consolidating small packs and removing unused chunks.
    /// Returns statistics about the repack operation.
    pub async fn repack(&self, max_pack_size: u64) -> Result<RepackStats> {
        let used_chunks = self.collect_used_chunks().await?;
        let repacker = Repacker::new(max_pack_size);

        // Collect pack information
        let pack_ids = self.list_packs().await?;
        let mut pack_infos = Vec::new();

        for pack_id in &pack_ids {
            let index = self.index.read().await;
            if let Some(info) = index.get_pack(pack_id) {
                pack_infos.push((pack_id.clone(), info.size));
            }
        }

        // Find packs that need repacking
        let candidates = repacker.find_repack_candidates(&pack_infos);

        if candidates.is_empty() {
            return Ok(RepackStats::default());
        }

        let mut stats = RepackStats {
            packs_read: candidates.len(),
            ..Default::default()
        };

        // Load all candidate packs and extract used chunks
        let mut chunks_to_repack: Vec<(ChunkID, Vec<u8>)> = Vec::new();

        for pack_id in &candidates {
            let pack = self.load_pack(pack_id).await?;
            stats.bytes_before += pack.size() as u64;

            // Get only the chunks that are still in use
            for chunk_id in pack.chunks.keys() {
                if used_chunks.contains(chunk_id) {
                    let chunk_data = pack.get_chunk(chunk_id)?;
                    chunks_to_repack.push((*chunk_id, chunk_data.to_vec()));
                }
            }
        }

        stats.chunks_copied = chunks_to_repack.len();

        // Create new packs with the used chunks
        let mut pack_manager = PackManager::new(max_pack_size);
        let mut new_packs = Vec::new();

        for (chunk_id, data) in chunks_to_repack {
            if let Some(finished_pack) = pack_manager.add_chunk(chunk_id, &data)? {
                new_packs.push(finished_pack);
            }
        }

        // Get the final pack if there's remaining data
        if let Some(final_pack) = pack_manager.finish_current_pack() {
            new_packs.push(final_pack);
        }

        // Save new packs and update index
        for pack in &new_packs {
            self.save_pack(pack).await?;

            for (chunk_id, chunk_entry) in &pack.chunks {
                self.save_chunk_location(
                    chunk_id,
                    &pack.header.pack_id,
                    chunk_entry.offset,
                    chunk_entry.length,
                )
                .await?;
            }

            stats.bytes_after += pack.size() as u64;
            stats.packs_written += 1;
        }

        // Delete old packs
        for pack_id in candidates {
            self.delete_pack(&pack_id).await?;
        }

        // Save index
        self.save_index().await?;

        Ok(stats)
    }

    /// Migrates the repository to a new format version if needed.
    /// Returns true if migration was performed.
    pub async fn migrate(&self) -> Result<bool> {
        // Currently only version 1 is supported
        if self.config.version == 1 {
            tracing::info!(
                "Repository already at latest version ({})",
                self.config.version
            );
            return Ok(false);
        }

        // Future version migrations would go here
        // For example:
        // if self.config.version == 1 {
        //     self.migrate_v1_to_v2().await?;
        // }

        Err(Error::InvalidFormatVersion {
            version: self.config.version,
        })
    }

    /// Clones the repository to a new location.
    /// This creates a complete copy of all repository data.
    pub async fn clone_to<P: AsRef<Path>>(&self, target_path: P) -> Result<CloneStats> {
        let target_path = target_path.as_ref();

        if target_path.exists() {
            return Err(Error::RepositoryExists {
                path: target_path.display().to_string(),
            });
        }

        // Create target directory structure
        fs::create_dir_all(target_path).await?;
        fs::create_dir_all(target_path.join("data")).await?;
        fs::create_dir_all(target_path.join("index")).await?;
        fs::create_dir_all(target_path.join("snapshots")).await?;
        fs::create_dir_all(target_path.join("keys")).await?;
        fs::create_dir_all(target_path.join("locks")).await?;

        let mut stats = CloneStats::default();

        // Copy config
        let config_data = self.storage.read("config").await?;
        fs::write(target_path.join("config"), &config_data).await?;
        stats.files_copied += 1;

        // Copy keys
        for key_name in self.storage.list("keys").await? {
            let data = self.storage.read(&format!("keys/{}", key_name)).await?;
            fs::write(target_path.join("keys").join(&key_name), &data).await?;
            stats.files_copied += 1;
        }

        // Copy index
        for index_name in self.storage.list("index").await? {
            let data = self.storage.read(&format!("index/{}", index_name)).await?;
            let size = data.len() as u64;
            fs::write(target_path.join("index").join(&index_name), &data).await?;
            stats.files_copied += 1;
            stats.bytes_copied += size;
        }

        // Copy data (packs and trees)
        for data_name in self.storage.list("data").await? {
            let data = self.storage.read(&format!("data/{}", data_name)).await?;
            let size = data.len() as u64;
            fs::write(target_path.join("data").join(&data_name), &data).await?;
            stats.files_copied += 1;
            stats.bytes_copied += size;

            if data_name.ends_with(".pack") {
                stats.packs_copied += 1;
            }
        }

        // Copy snapshots
        for snapshot_name in self.storage.list("snapshots").await? {
            let data = self
                .storage
                .read(&format!("snapshots/{}", snapshot_name))
                .await?;
            fs::write(target_path.join("snapshots").join(&snapshot_name), &data).await?;
            stats.files_copied += 1;
            stats.snapshots_copied += 1;
        }

        tracing::info!(
            "Cloned repository: {} files, {} bytes, {} packs, {} snapshots",
            stats.files_copied,
            stats.bytes_copied,
            stats.packs_copied,
            stats.snapshots_copied
        );

        Ok(stats)
    }

    /// Verifies the integrity of the repository.
    /// Returns (valid_packs, corrupt_packs, valid_chunks, corrupt_chunks).
    pub async fn verify(&self, check_data: bool) -> Result<VerifyStats> {
        let mut stats = VerifyStats::default();

        let pack_ids = self.list_packs().await?;

        for pack_id in pack_ids {
            match self.load_pack(&pack_id).await {
                Ok(pack) => {
                    if check_data {
                        // Verify checksum - only count as valid if checksum passes
                        if pack.verify_checksum()? {
                            stats.valid_packs += 1;
                            stats.valid_chunks += pack.chunks.len();
                        } else {
                            stats.corrupt_packs += 1;
                            stats.corrupt_chunks += pack.chunks.len();
                            tracing::warn!("Pack {} has invalid checksum", pack_id);
                        }
                    } else {
                        // Not checking data, count as valid if loadable
                        stats.valid_packs += 1;
                        stats.valid_chunks += pack.chunks.len();
                    }
                }
                Err(e) => {
                    stats.corrupt_packs += 1;
                    tracing::warn!("Failed to load pack {}: {}", pack_id, e);
                }
            }
        }

        // Verify snapshots
        let snapshot_ids = self.list_snapshots().await?;
        for snapshot_id in &snapshot_ids {
            match self.load_snapshot(snapshot_id).await {
                Ok(_) => stats.valid_snapshots += 1,
                Err(e) => {
                    stats.corrupt_snapshots += 1;
                    tracing::warn!("Failed to load snapshot {}: {}", snapshot_id, e);
                }
            }
        }

        Ok(stats)
    }
}

/// Clone operation statistics.
#[derive(Debug, Default)]
pub struct CloneStats {
    pub files_copied: usize,
    pub bytes_copied: u64,
    pub packs_copied: usize,
    pub snapshots_copied: usize,
}

/// Verify operation statistics.
#[derive(Debug, Default)]
pub struct VerifyStats {
    pub valid_packs: usize,
    pub corrupt_packs: usize,
    pub valid_chunks: usize,
    pub corrupt_chunks: usize,
    pub valid_snapshots: usize,
    pub corrupt_snapshots: usize,
}

/// Repository statistics.
#[derive(Debug)]
pub struct RepoStats {
    pub chunk_count: usize,
    pub pack_count: usize,
}

/// Compaction statistics.
#[derive(Debug)]
pub struct CompactStats {
    pub chunks_removed: usize,
    pub packs_removed: usize,
    pub bytes_freed: u64,
}

/// Pack cache statistics.
#[derive(Debug)]
pub struct CacheStats {
    /// Number of packs currently cached
    pub pack_count: usize,
    /// Total size of cached packs in bytes
    pub total_size: usize,
    /// Maximum cache size in bytes
    pub max_size: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyFile {
    encrypted_key: Vec<u8>,
    kdf_params: crate::KdfParams,
}
