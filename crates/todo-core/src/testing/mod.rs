//! Test-Hilfen für dieses und andere Crates (Feature `test-support`).

mod in_memory_remote_store;
mod in_memory_repository;
pub mod remote_store_contract;
pub mod repository_contract;
mod sync_contract;

pub use in_memory_remote_store::{Fault, InMemoryRemoteStore, RemoteCall};
pub use in_memory_repository::InMemoryTaskRepository;
