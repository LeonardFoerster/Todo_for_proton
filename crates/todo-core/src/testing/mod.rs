//! Test-Hilfen für dieses und andere Crates (Feature `test-support`).

mod in_memory_repository;
pub mod repository_contract;
mod sync_contract;

pub use in_memory_repository::InMemoryTaskRepository;
