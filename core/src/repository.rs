use crate::{Error, Result, RepoConfig, crypto::{MasterKey, Encryptor}};
use crate::{SnapshotID, ChunkID, PackID};
use crate::snapshot::{Snapshot, Tree};
use crate::pack::PackFile;
use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Serialize, Deserialize};
use bytes::Bytes;

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
/// ├── index/          # Chunk location index
/// ├── snapshots/      # Snapshot metadata
/// └── locks/          # Repository locks
/// ```
///
/// # Examples
///
/// ```no_run
/// use ghostsnap_core::Repository;
///
/// #[tokio::main]
/// async fn main() -> ghostsnap_core::Result<()> {
///     // Initialize a new repository
///     let repo = Repository::init("./backup-repo", "my-password").await?;
///
///     // Open an existing repository
///     let repo = Repository::open("./backup-repo", "my-password").await?;
///
///     Ok(())
/// }
/// ```
pub struct Repository {
    path: PathBuf,
    config: RepoConfig,
    #[allow(dead_code)] // Used for key rotation in future
    master_key: Option<MasterKey>,
    encryptor: Option<Encryptor>,
}

impl Repository {
    /// Initializes a new repository at the given path.
    ///
    /// This creates the repository directory structure, generates encryption keys,
    /// and stores the encrypted configuration. The repository is immediately ready for use.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path where the repository should be created
    /// * `password` - The master password used to encrypt the repository's data keys
    ///
    /// # Returns
    ///
    /// Returns a `Repository` instance ready for backup operations.
    ///
    /// # Errors
    ///
    /// Returns `Error::RepositoryExists` if a repository already exists at the path.
    /// Returns `Error::Io` if filesystem operations fail.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use ghostsnap_core::Repository;
    /// # #[tokio::main]
    /// # async fn main() -> ghostsnap_core::Result<()> {
    /// let repo = Repository::init("./my-backups", "strong-password").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn init<P: AsRef<Path>>(path: P, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if path.exists() {
            let config_path = path.join("config");
            if config_path.exists() {
                return Err(Error::RepositoryExists {
                    path: path.display().to_string(),
                });
            }
        }
        
        fs::create_dir_all(&path).await?;
        fs::create_dir_all(path.join("data")).await?;
        fs::create_dir_all(path.join("index")).await?;
        fs::create_dir_all(path.join("snapshots")).await?;
        fs::create_dir_all(path.join("keys")).await?;
        fs::create_dir_all(path.join("locks")).await?;
        
        let config = RepoConfig::default();
        
        let master_key = MasterKey::derive_from_password(
            password,
            &config.kdf_params.salt,
            &config.kdf_params,
        )?;
        
        let data_key = MasterKey::generate();
        let encryptor = Encryptor::new(data_key.as_bytes())?;
        
        let key_encryptor = Encryptor::new(master_key.as_bytes())?;
        let encrypted_data_key = key_encryptor.encrypt(data_key.as_bytes())?;
        
        let key_file = KeyFile {
            encrypted_key: encrypted_data_key,
            kdf_params: config.kdf_params.clone(),
        };
        
        let config_json = serde_json::to_string_pretty(&config)?;
        fs::write(path.join("config"), config_json).await?;
        
        let key_json = serde_json::to_string_pretty(&key_file)?;
        let key_id = uuid::Uuid::new_v4().to_string();
        fs::write(path.join("keys").join(&key_id), key_json).await?;
        
        Ok(Self {
            path,
            config,
            master_key: Some(master_key),
            encryptor: Some(encryptor),
        })
    }
    
    /// Opens an existing repository.
    ///
    /// Loads the repository configuration and decrypts the data keys using the provided password.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the repository
    /// * `password` - The master password for decrypting the repository keys
    ///
    /// # Returns
    ///
    /// Returns a `Repository` instance ready for operations.
    ///
    /// # Errors
    ///
    /// * `Error::RepositoryNotFound` - Repository doesn't exist at the path
    /// * `Error::InvalidPassword` - Incorrect password provided
    /// * `Error::InvalidFormatVersion` - Unsupported repository version
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use ghostsnap_core::Repository;
    /// # #[tokio::main]
    /// # async fn main() -> ghostsnap_core::Result<()> {
    /// let repo = Repository::open("./my-backups", "strong-password").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open<P: AsRef<Path>>(path: P, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        
        if !path.exists() {
            return Err(Error::RepositoryNotFound {
                path: path.display().to_string(),
            });
        }
        
        let config_data = fs::read_to_string(path.join("config")).await?;
        let config: RepoConfig = serde_json::from_str(&config_data)?;
        
        if config.version != 1 {
            return Err(Error::InvalidFormatVersion {
                version: config.version,
            });
        }
        
        let keys_dir = path.join("keys");
        let mut key_entries = fs::read_dir(&keys_dir).await?;
        let mut key_file = None;
        
        while let Some(entry) = key_entries.next_entry().await? {
            let key_data = fs::read_to_string(entry.path()).await?;
            if let Ok(kf) = serde_json::from_str::<KeyFile>(&key_data) {
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
        let data_key = key_encryptor.decrypt(&key_file.encrypted_key)
            .map_err(|_| Error::InvalidPassword)?;
        
        let encryptor = Encryptor::new(&data_key)?;
        
        Ok(Self {
            path,
            config,
            master_key: Some(master_key),
            encryptor: Some(encryptor),
        })
    }
    
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    pub fn config(&self) -> &RepoConfig {
        &self.config
    }
    
    pub fn encryptor(&self) -> Result<&Encryptor> {
        self.encryptor.as_ref()
            .ok_or_else(|| Error::Other("Repository not unlocked".to_string()))
    }

    pub async fn save_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        let encryptor = self.encryptor()?;
        let data = snapshot.serialize(encryptor)?;
        let snapshot_path = self.path.join("snapshots").join(&snapshot.id);
        fs::write(snapshot_path, data).await?;
        Ok(())
    }

    pub async fn load_snapshot(&self, snapshot_id: &SnapshotID) -> Result<Snapshot> {
        let encryptor = self.encryptor()?;
        let snapshot_path = self.path.join("snapshots").join(snapshot_id);
        let data = fs::read(snapshot_path).await?;
        Snapshot::deserialize(&data, encryptor)
    }

    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotID>> {
        let snapshots_dir = self.path.join("snapshots");
        let mut entries = fs::read_dir(snapshots_dir).await?;
        let mut snapshot_ids = Vec::new();
        
        while let Some(entry) = entries.next_entry().await? {
            if let Some(file_name) = entry.file_name().to_str() {
                snapshot_ids.push(file_name.to_string());
            }
        }
        
        Ok(snapshot_ids)
    }

    pub async fn save_tree(&self, tree: &Tree) -> Result<ChunkID> {
        let encryptor = self.encryptor()?;
        let data = tree.serialize(encryptor)?;
        let tree_id = ChunkID::from_data(&data);
        let tree_path = self.path.join("data").join(tree_id.to_hex());
        fs::write(tree_path, data).await?;
        Ok(tree_id)
    }

    pub async fn load_tree(&self, tree_id: &ChunkID) -> Result<Tree> {
        let encryptor = self.encryptor()?;
        let tree_path = self.path.join("data").join(tree_id.to_hex());
        let data = fs::read(tree_path).await?;
        Tree::deserialize(&data, encryptor)
    }

    pub async fn save_pack(&self, pack: &PackFile) -> Result<()> {
        let encryptor = self.encryptor()?;
        let pack_path = self.path.join("data").join(format!("{}.pack", pack.header.pack_id));
        let mut file = fs::File::create(pack_path).await?;
        pack.write_to(&mut file, encryptor).await?;
        Ok(())
    }

    pub async fn load_pack(&self, pack_id: &PackID) -> Result<PackFile> {
        let encryptor = self.encryptor()?;
        let pack_path = self.path.join("data").join(format!("{}.pack", pack_id));
        let mut file = fs::File::open(pack_path).await?;
        PackFile::read_from(&mut file, encryptor).await
    }

    pub async fn has_chunk(&self, chunk_id: &ChunkID) -> Result<bool> {
        let index_path = self.path.join("index").join(chunk_id.to_hex());
        Ok(index_path.exists())
    }

    pub async fn save_chunk_location(&self, chunk_id: &ChunkID, pack_id: &PackID, offset: u64, length: u32) -> Result<()> {
        let location = ChunkLocation {
            pack_id: pack_id.clone(),
            offset,
            length,
        };
        let location_data = serde_json::to_vec(&location)?;
        let index_path = self.path.join("index").join(chunk_id.to_hex());
        fs::write(index_path, location_data).await?;
        Ok(())
    }

    pub async fn load_chunk_location(&self, chunk_id: &ChunkID) -> Result<ChunkLocation> {
        let index_path = self.path.join("index").join(chunk_id.to_hex());
        let data = fs::read(index_path).await?;
        let location: ChunkLocation = serde_json::from_slice(&data)?;
        Ok(location)
    }

    pub async fn load_chunk(&self, chunk_id: &ChunkID) -> Result<Bytes> {
        let location = self.load_chunk_location(chunk_id).await?;
        let pack = self.load_pack(&location.pack_id).await?;
        pack.get_chunk(chunk_id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyFile {
    encrypted_key: Vec<u8>,
    kdf_params: crate::KdfParams,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChunkLocation {
    pub pack_id: PackID,
    pub offset: u64,
    pub length: u32,
}