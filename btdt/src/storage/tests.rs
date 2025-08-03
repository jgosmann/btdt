use crate::storage::Storage;
use std::io;
use std::io::{Read, Write};

#[macro_export]
macro_rules! test_storage {
    ($mod_name:ident, $constructor:expr) => {
        mod $mod_name {
            use super::*;
            use crate::storage::tests::{read_file_from_storage_to_string, write_file_to_storage};
            #[allow(unused_imports)] // false positive
            use std::io::{Read, Write};

            #[test]
            fn test_get_returns_error_for_non_existent_file() {
                let storage = $constructor;
                let result = storage.get("/non-existent-file.txt");
                assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
            }

            #[test]
            fn test_list_returns_error_for_non_existent_dir() {
                let storage = $constructor;
                let result = storage.list("/non-existent-dir");
                assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
            }

            #[test]
            fn test_list_returns_error_for_non_existent_file_or_dir() {
                let storage = $constructor;
                let result = storage.delete("/non-existent");
                assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
            }

            #[test]
            fn test_can_get_file_that_was_previously_put_into_storage() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/dir/file.txt", "Hello, world!").unwrap();
                assert_eq!(
                    &read_file_from_storage_to_string(&storage, "/dir/file.txt").unwrap(),
                    "Hello, world!"
                );
            }

            #[test]
            fn test_different_files_are_separate() {
                let storage = $constructor;

                write_file_to_storage(&storage, "/a.txt", "Hello, a!").unwrap();
                write_file_to_storage(&storage, "/b.txt", "Hello, b!").unwrap();

                assert_eq!(
                    &read_file_from_storage_to_string(&storage, "/a.txt").unwrap(),
                    "Hello, a!"
                );
                assert_eq!(
                    &read_file_from_storage_to_string(&storage, "/b.txt").unwrap(),
                    "Hello, b!"
                );
            }

            #[test]
            fn test_can_overwrite_existing_file() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/file.txt", "Hello, world!").unwrap();
                write_file_to_storage(&storage, "/file.txt", "Bye, world!").unwrap();
                assert_eq!(
                    &read_file_from_storage_to_string(&storage, "/file.txt").unwrap(),
                    "Bye, world!"
                );
            }

            #[test]
            fn test_errors_when_trying_to_overwrite_dir_with_file() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/dir/file.txt", "file-content").unwrap();
                assert!(storage.put("dir").is_err());
            }

            #[test]
            fn test_list_returns_direct_children_of_directory() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/rootfile.txt", "rootfile-content").unwrap();
                write_file_to_storage(&storage, "/dir/file1.txt", "file1-content").unwrap();
                write_file_to_storage(&storage, "/dir/file2.txt", "file2-content").unwrap();
                write_file_to_storage(&storage, "/dir/subdir/subfile.txt", "subfile-content")
                    .unwrap();

                let mut entries: Vec<_> = storage.list("/").unwrap().map(Result::unwrap).collect();
                entries.sort_unstable_by_key(|entry| entry.name.to_string());
                assert_eq!(
                    entries,
                    vec![
                        StorageEntry {
                            entry_type: EntryType::Directory,
                            name: Cow::Owned("dir".to_string()),
                            size: 0,
                        },
                        StorageEntry {
                            entry_type: EntryType::File,
                            name: Cow::Owned("rootfile.txt".to_string()),
                            size: 16,
                        }
                    ]
                );

                let mut entries: Vec<_> =
                    storage.list("/dir").unwrap().map(Result::unwrap).collect();
                entries.sort_unstable_by_key(|entry| entry.name.to_string());
                assert_eq!(
                    entries,
                    vec![
                        StorageEntry {
                            entry_type: EntryType::File,
                            name: Cow::Owned("file1.txt".to_string()),
                            size: 13,
                        },
                        StorageEntry {
                            entry_type: EntryType::File,
                            name: Cow::Owned("file2.txt".to_string()),
                            size: 13,
                        },
                        StorageEntry {
                            entry_type: EntryType::Directory,
                            name: Cow::Owned("subdir".to_string()),
                            size: 0,
                        },
                    ]
                );
            }

            #[test]
            fn test_can_delete_file() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/file.txt", "file-content").unwrap();
                storage.delete("/file.txt").unwrap();
                assert_eq!(
                    storage.get("/file.txt").err().unwrap().kind(),
                    ErrorKind::NotFound
                );
                assert_eq!(storage.list("/").unwrap().count(), 0);
            }

            #[test]
            fn test_can_delete_empty_directory() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/dir/file.txt", "file-content").unwrap();
                storage.delete("/dir/file.txt").unwrap();
                storage.delete("/dir").unwrap();
                assert_eq!(storage.list("/").unwrap().count(), 0);
            }

            #[test]
            fn test_delete_returns_error_for_non_empty_dir() {
                let storage = $constructor;
                write_file_to_storage(&storage, "/dir/file.txt", "file-content").unwrap();
                assert_eq!(
                    storage.delete("/dir").err().unwrap().kind(),
                    ErrorKind::DirectoryNotEmpty
                );
            }

            #[test]
            fn test_exists_returns_true_for_existing_file() {
                let storage = $constructor;
                assert!(!storage.exists_file("/dir").unwrap());
                assert!(!storage.exists_file("/dir/file.txt").unwrap());
                write_file_to_storage(&storage, "/dir/file.txt", "file-content").unwrap();
                assert!(!storage.exists_file("/dir").unwrap());
                assert!(storage.exists_file("/dir/file.txt").unwrap());
            }

            #[test]
            fn test_put_is_atomic() {
                let storage_a = $constructor;
                let storage_b = $constructor;
                let mut writer_a = storage_a.put("/file.txt").unwrap();
                let mut writer_b = storage_b.put("/file.txt").unwrap();
                writer_a.write_all(b"Hello, ").unwrap();
                writer_a.flush().unwrap();
                writer_b.write_all(b"Goodbye, ").unwrap();
                writer_b.flush().unwrap();
                writer_a.write_all(b"world!").unwrap();
                writer_a.flush().unwrap();
                writer_b.write_all(b"world!").unwrap();
                writer_b.flush().unwrap();
                drop(writer_a);
                drop(writer_b);
                let content_a = &read_file_from_storage_to_string(&storage_a, "/file.txt").unwrap();
                let content_b = &read_file_from_storage_to_string(&storage_a, "/file.txt").unwrap();
                assert!(content_a == "Hello, world!" || content_a == "Goodbye, world!");
                assert!(content_b == "Hello, world!" || content_b == "Goodbye, world!");
            }
        }
    };
}

pub fn write_file_to_storage(storage: &impl Storage, path: &str, content: &str) -> io::Result<()> {
    let mut writer = storage.put(path)?;
    writer.write_all(content.as_bytes())
}

pub fn read_file_from_storage_to_string(storage: &impl Storage, path: &str) -> io::Result<String> {
    let mut reader = storage.get(path)?;
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    Ok(buf)
}
