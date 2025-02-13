use super::{Ballot, Entry};
use crate::Index;
use anyhow::Result;
use rocksdb::{ColumnFamilyDescriptor, Options, DB};
use std::cmp::Ordering;
use std::path::Path;

const CF_ENTRY: &str = "entry";
const BALLOT_KEY: &str = "ballot";
const CMP: &str = "index_asc";

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct IndexKey(u64);
fn encode_index(i: Index) -> Vec<u8> {
    bincode::serialize(&IndexKey(i)).unwrap()
}
fn decode_index(s: &[u8]) -> Index {
    let x: IndexKey = bincode::deserialize(s).unwrap();
    x.0
}
fn comparator_fn(x: &[u8], y: &[u8]) -> Ordering {
    let x: Index = decode_index(x);
    let y: Index = decode_index(y);
    x.cmp(&y)
}

impl Storage {
    pub fn destroy(path: &Path) -> Result<()> {
        let opts = Options::default();
        DB::destroy(&opts, path)?;
        Ok(())
    }
    /// Create the initial state.
    /// You should call `destory` before calling this function.
    pub fn create(path: &Path) -> Result<()> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        let mut opts = Options::default();
        opts.set_comparator(CMP, comparator_fn);
        let cf_descs = vec![ColumnFamilyDescriptor::new(CF_ENTRY, opts)];
        let db = DB::open_cf_descriptors(&db_opts, path, cf_descs)?;

        let initial_ballot = Ballot {
            cur_term: 0,
            voted_for: None,
        };
        let b: Vec<u8> = initial_ballot.into();
        db.put(BALLOT_KEY, b)?;

        Ok(())
    }
    fn open_db(path: &Path) -> Result<DB> {
        let db_opts = Options::default();
        let mut opts = Options::default();
        opts.set_comparator(CMP, comparator_fn);
        let cf_descs = vec![ColumnFamilyDescriptor::new(CF_ENTRY, opts)];
        let db = DB::open_cf_descriptors(&db_opts, path, cf_descs)?;
        Ok(db)
    }
    pub fn open(path: &Path) -> Result<Self> {
        let db = Self::open_db(path)?;
        Ok(Self { db })
    }
}
pub struct Storage {
    db: DB,
}
#[async_trait::async_trait]
impl super::RaftStorage for Storage {
    async fn delete_entry(&self, i: Index) -> Result<()> {
        let cf = self.db.cf_handle(CF_ENTRY).unwrap();
        self.db.delete_cf(cf, encode_index(i))?;
        Ok(())
    }
    async fn get_head_index(&self) -> Result<Index> {
        let cf = self.db.cf_handle(CF_ENTRY).unwrap();
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_first();
        // The iterator is empty
        if !iter.valid() {
            return Ok(0);
        }
        let key = iter.key().unwrap();
        let v = decode_index(key);
        Ok(v)
    }
    async fn get_last_index(&self) -> Result<Index> {
        let cf = self.db.cf_handle(CF_ENTRY).unwrap();
        let mut iter = self.db.raw_iterator_cf(cf);
        iter.seek_to_last();
        // The iterator is empty
        if !iter.valid() {
            return Ok(0);
        }
        let key = iter.key().unwrap();
        let v = decode_index(key);
        Ok(v)
    }
    async fn insert_entry(&self, i: Index, e: Entry) -> Result<()> {
        let cf = self.db.cf_handle(CF_ENTRY).unwrap();
        let b: Vec<u8> = e.into();
        self.db.put_cf(&cf, encode_index(i), b)?;
        Ok(())
    }
    async fn get_entry(&self, i: Index) -> Result<Option<Entry>> {
        let cf = self.db.cf_handle(CF_ENTRY).unwrap();
        let b: Option<Vec<u8>> = self.db.get_cf(&cf, encode_index(i))?;
        Ok(b.map(|x| x.into()))
    }
    async fn save_ballot(&self, v: Ballot) -> Result<()> {
        let b: Vec<u8> = v.into();
        self.db.put(BALLOT_KEY, b)?;
        Ok(())
    }
    async fn load_ballot(&self) -> Result<Ballot> {
        let b = self.db.get(BALLOT_KEY)?.unwrap();
        let v = b.into();
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;

    use super::*;
    use crate::storage;
    #[tokio::test]
    #[serial]
    async fn test_rocksdb_storage() -> Result<()> {
        let _ = std::fs::create_dir("/tmp/lol");
        let path = Path::new("/tmp/lol/disk.db");
        Storage::destroy(path)?;
        Storage::create(path)?;

        let s = Storage::open(path)?;
        storage::test_storage(s).await?;

        Storage::destroy(path)?;
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_rocksdb_persistency() -> Result<()> {
        let _ = std::fs::create_dir("/tmp/lol");
        let path = Path::new("/tmp/lol/disk.db");
        Storage::destroy(path)?;
        Storage::create(path)?;

        let s = Storage::open(path)?;
        storage::persistency::test_pre_close(s).await?;

        let s = Storage::open(path)?;
        storage::persistency::test_post_close(s).await?;

        Storage::destroy(path)?;

        Ok(())
    }
}
