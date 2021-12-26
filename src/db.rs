use crate::multimap_table::MultiMapTable;
use crate::table::Table;
use crate::tree_store::{DbStats, Storage, TableType};
use crate::types::{RedbKey, RedbValue};
use crate::Error;
use memmap2::MmapMut;
use std::fs::OpenOptions;
use std::path::Path;

pub struct Database {
    storage: Storage,
}

impl Database {
    /// Opens the specified file as a redb database.
    /// * if the file does not exist, or is an empty file, a new database will be initialized in it
    /// * if the file is a valid redb database, it will be opened
    /// * otherwise this function will return an error
    ///
    /// `db_size`: the maximum size in bytes of the database. Note: this cannot be changed after the
    /// database is created.
    /// TODO: remove the restriction that db_size cannot be changed
    ///
    /// # Safety
    ///
    /// The file referenced by `path` must only be concurrently modified by compatible versions
    /// of redb
    pub unsafe fn open(path: impl AsRef<Path>, db_size: usize) -> Result<Database, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        file.set_len(db_size as u64)?;

        let mmap = MmapMut::map_mut(&file)?;
        let storage = Storage::new(mmap, None)?;
        Ok(Database { storage })
    }

    pub fn open_table<K: RedbKey + ?Sized, V: RedbValue + ?Sized>(
        &self,
        name: impl AsRef<[u8]>,
    ) -> Result<Table<K, V>, Error> {
        assert!(!name.as_ref().is_empty());
        // TODO: this could conflict with an on-going write
        let id = self.storage.allocate_write_transaction();
        let (definition, root) = self.storage.get_or_create_table(
            name.as_ref(),
            TableType::Normal,
            id,
            self.storage.get_root_page_number(),
        )?;
        self.storage.commit(Some(root), id)?;
        Table::new(definition.get_id(), &self.storage)
    }

    pub fn open_multimap_table<K: RedbKey + ?Sized, V: RedbKey + ?Sized>(
        &self,
        name: impl AsRef<[u8]>,
    ) -> Result<MultiMapTable<K, V>, Error> {
        assert!(!name.as_ref().is_empty());
        // TODO: this could conflict with an on-going write
        let id = self.storage.allocate_write_transaction();
        let (definition, root) = self.storage.get_or_create_table(
            name.as_ref(),
            TableType::MultiMap,
            id,
            self.storage.get_root_page_number(),
        )?;
        self.storage.commit(Some(root), id)?;
        MultiMapTable::new(definition.get_id(), &self.storage)
    }

    pub fn stats(&self) -> Result<DbStats, Error> {
        self.storage.storage_stats()
    }

    pub fn print_debug(&self) {
        self.storage.print_debug()
    }
}

pub struct DatabaseBuilder {
    page_size: Option<usize>,
}

impl DatabaseBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { page_size: None }
    }

    pub fn set_page_size(&mut self, size: usize) -> &mut Self {
        assert!(size.is_power_of_two());
        self.page_size = Some(size);
        self
    }

    /// Opens the specified file as a redb database.
    /// * if the file does not exist, or is an empty file, a new database will be initialized in it
    /// * if the file is a valid redb database, it will be opened
    /// * otherwise this function will return an error
    ///
    /// `db_size`: the maximum size in bytes of the database. Note: this cannot be changed after the
    /// database is created.
    /// TODO: remove the restriction that db_size cannot be changed
    ///
    /// # Safety
    ///
    /// The file referenced by `path` must only be concurrently modified by compatible versions
    /// of redb
    pub unsafe fn open(self, path: impl AsRef<Path>, db_size: usize) -> Result<Database, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        file.set_len(db_size as u64)?;

        let mmap = MmapMut::map_mut(&file)?;
        let storage = Storage::new(mmap, self.page_size)?;
        Ok(Database { storage })
    }
}

#[cfg(test)]
mod test {
    use crate::{Database, Table};
    use tempfile::NamedTempFile;

    #[test]
    fn non_page_size_multiple() {
        let tmpfile: NamedTempFile = NamedTempFile::new().unwrap();

        let db_size = 1024 * 1024 + 1;
        let db = unsafe { Database::open(tmpfile.path(), db_size).unwrap() };
        let mut table: Table<[u8], [u8]> = db.open_table("x").unwrap();

        let key = vec![0; 1024];
        let value = vec![0; 1];
        let mut txn = table.begin_write().unwrap();
        txn.insert(&key, &value).unwrap();
        txn.commit().unwrap();

        let read_txn = table.read_transaction().unwrap();
        assert_eq!(read_txn.len().unwrap(), 1);
    }
}
