use crate::config::CacheConfig;
use btdt::cache::cache_dispatcher::CacheDispatcher;
use btdt::storage::filesystem::FilesystemStorage;
use btdt::storage::in_memory::InMemoryStorage;

#[derive(Clone)]
pub enum StorageHandle {
    InMemory(InMemoryStorage),
    Filesystem(FilesystemStorage),
}

impl From<&CacheConfig> for StorageHandle {
    fn from(cache_config: &CacheConfig) -> Self {
        match cache_config {
            CacheConfig::InMemory => StorageHandle::InMemory(InMemoryStorage::new()),
            CacheConfig::Filesystem { path } => {
                StorageHandle::Filesystem(FilesystemStorage::new(path.into()))
            }
        }
    }
}

impl StorageHandle {
    pub fn into_cache(self) -> CacheDispatcher {
        match self {
            StorageHandle::InMemory(storage) => {
                CacheDispatcher::InMemory(btdt::cache::local::LocalCache::new(storage))
            }
            StorageHandle::Filesystem(storage) => {
                CacheDispatcher::Filesystem(btdt::cache::local::LocalCache::new(storage))
            }
        }
    }

    pub fn to_cache(&self) -> CacheDispatcher {
        self.clone().into_cache()
    }
}
