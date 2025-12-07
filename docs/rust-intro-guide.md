# Rust Programming - Beginner's Guide

A practical introduction to Rust using real examples from the Metrics Collector project.

## Table of Contents

1. [Introduction to Rust](#introduction-to-rust)
2. [Basic Syntax](#basic-syntax)
3. [Ownership and Borrowing](#ownership-and-borrowing)
4. [Structs and Implementations](#structs-and-implementations)
5. [Enums and Pattern Matching](#enums-and-pattern-matching)
6. [Traits (Interfaces)](#traits-interfaces)
7. [Error Handling](#error-handling)
8. [Async/Await](#asyncawait)
9. [Modules and Organization](#modules-and-organization)
10. [Common Patterns in This Project](#common-patterns-in-this-project)

---

## Introduction to Rust

### What is Rust?

Rust is a systems programming language that focuses on:
- **Safety**: No null pointers, no data races
- **Speed**: Zero-cost abstractions, as fast as C/C++
- **Concurrency**: Fearless concurrency through ownership

### Why Rust for This Project?

Our metrics collector uses Rust because:
- **Memory Safety**: No crashes from memory errors
- **Performance**: Minimal CPU and memory usage
- **Async/Await**: Efficient concurrent metric collection
- **Type Safety**: Errors caught at compile time

---

## Basic Syntax

### Variables and Mutability

In Rust, variables are **immutable by default**:

```rust
// From src/scheduler.rs
let metric_name = collector.name().to_string();
// metric_name cannot be changed

// To make it mutable, use 'mut'
let mut success_count = 0;
success_count += 1;  // This is allowed
```

**Why?** Immutability prevents bugs and makes code easier to reason about.

### Data Types

Rust has strong, static typing:

```rust
// From src/metrics/load_average.rs
let cpu_count = num_cpus::get();           // usize
let load_avg_one = 1.5;                    // f64 (default for floats)
let timeout: u64 = 5;                      // unsigned 64-bit integer
let name: &str = "LoadAverage";            // string slice (borrowed)
let owned_string: String = String::from("test");  // owned string
```

**Common Types:**
- `i32`, `i64`: Signed integers
- `u32`, `u64`: Unsigned integers
- `f32`, `f64`: Floating point numbers
- `bool`: Boolean (true/false)
- `String`: Owned, growable string
- `&str`: String slice (borrowed reference)

### Functions

```rust
// From src/metrics/disk.rs

// Function that takes a u64 and returns an i64
fn bytes_to_mb(bytes: u64) -> i64 {
    (bytes / (1024 * 1024)) as i64
}

// Function with multiple parameters
fn calculate_percentage(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}

// No return type means it returns () (unit type, like void)
fn log_message() {
    println!("Hello!");
}
```

**Key Points:**
- Last expression is the return value (no `return` needed)
- `;` suppresses the return value
- Type annotations are mandatory for parameters

---

## Ownership and Borrowing

This is Rust's most unique feature - it ensures memory safety without garbage collection.

### Ownership Rules

1. Each value has one owner
2. When the owner goes out of scope, the value is dropped
3. Only one owner at a time

```rust
// From src/scheduler.rs

// collectors is owned by this function
pub async fn start(self, collectors: Vec<Box<dyn MetricCollector>>) {
    // collectors is moved into the for loop
    for collector in collectors {
        // Each collector is now owned by this iteration
        // At the end of the iteration, collector is dropped
    }
    // collectors is no longer accessible here (it was moved)
}
```

### Borrowing (References)

Instead of transferring ownership, you can **borrow** a reference:

```rust
// From src/storage.rs

// &self borrows 'self' immutably
pub async fn store_metric(
    &self,                      // Immutable borrow
    collection_name: &str,      // Borrowed string slice
    document: Document,         // Owned value
) -> Result<(), StorageError> {
    // We can read self but not modify it
    let db = self.client.database(&self.database_name);
    Ok(())
}

// &mut self borrows 'self' mutably
pub fn refresh_data(&mut self) {
    // We can modify self
    self.last_updated = Utc::now();
}
```

**Borrowing Rules:**
- You can have unlimited immutable borrows (`&T`)
- OR exactly one mutable borrow (`&mut T`)
- But not both at the same time

### Arc - Atomic Reference Counting

When you need multiple owners (like in async tasks):

```rust
// From src/scheduler.rs

use std::sync::Arc;

// Create Arc (Atomic Reference Counted pointer)
let settings = Arc::new(settings);
let storage = Arc::new(storage);

// Clone creates a new reference, not a copy of the data
for collector in collectors {
    let settings = Arc::clone(&settings);  // Cheap: just increments counter
    let storage = Arc::clone(&storage);    // Cheap: just increments counter

    tokio::spawn(async move {
        // This task owns its cloned Arc references
        // Data is shared safely between tasks
    });
}
```

**Why Arc?**
- Share data between async tasks
- Thread-safe reference counting
- Data is dropped when last reference goes out of scope

---

## Structs and Implementations

### Defining Structs

Structs are like classes in other languages:

```rust
// From src/config.rs

pub struct ConfigManager {
    /// MongoDB client instance
    client: Client,

    /// Database name
    database_name: String,
}

pub struct MonitoringSettings {
    pub key: String,
    pub metric_settings: HashMap<String, MetricSettings>,
}
```

**Visibility:**
- No keyword = private (only accessible in same module)
- `pub` = public (accessible from anywhere)

### Implementation Blocks

```rust
// From src/config.rs

impl ConfigManager {
    // Associated function (like a static method)
    // Called as: ConfigManager::new(...)
    pub async fn new(
        connection_string: &str,
        database_name: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let client = Client::with_uri_str(connection_string).await?;
        let database_name = database_name.unwrap_or("monitoring").to_string();

        // Self (with capital S) refers to ConfigManager
        Ok(Self {
            client,
            database_name,
        })
    }

    // Method (takes &self)
    // Called as: config_manager.load_settings(...)
    pub async fn load_settings(&self, key: &str) -> Result<MonitoringSettings, ConfigError> {
        // self (lowercase) refers to the instance
        let db = self.client.database(&self.database_name);
        // ...
    }
}
```

**Patterns:**
- `new()` - Constructor pattern
- `&self` - Borrow instance (can't modify)
- `&mut self` - Mutable borrow (can modify)
- `self` - Take ownership (consume the instance)

---

## Enums and Pattern Matching

### Enums

Enums can hold data (unlike in many languages):

```rust
// From src/config.rs

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("MongoDB connection failed: {0}")]
    MongoConnectionError(#[from] mongodb::error::Error),

    #[error("Settings not found for key: {0}")]
    SettingsNotFound(String),

    #[error("Invalid settings format: {0}")]
    InvalidSettings(String),
}
```

**Key Points:**
- Each variant can hold different types of data
- `#[derive(...)]` automatically implements traits
- `#[error(...)]` from `thiserror` creates error messages

### Pattern Matching

```rust
// From src/config.rs

match collection.find_one(filter, None).await? {
    Some(settings) => {
        info!("Successfully loaded settings");
        Ok(settings)
    }
    None => {
        warn!("No settings found");
        Err(ConfigError::SettingsNotFound(key.to_string()))
    }
}
```

**Match is powerful:**
- Must cover all cases (compiler enforces)
- Can destructure complex types
- More powerful than switch statements

### Option and Result

Two essential enums in Rust:

```rust
// Option - represents presence or absence of a value
pub enum Option<T> {
    Some(T),
    None,
}

// From src/main.rs
fn parse_arguments() -> Result<AppConfig> {
    let mongodb_uri = find_arg("--mongodb")
        .context("Missing required argument")?;  // ? early returns on None
}

// Result - represents success or error
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

// From src/storage.rs
pub async fn store_metric(
    &self,
    collection_name: &str,
    document: Document,
) -> Result<(), StorageError> {
    match collection.insert_one(document, None).await {
        Ok(result) => {
            debug!("Stored successfully");
            Ok(())
        }
        Err(e) => {
            error!("Storage failed: {}", e);
            Err(StorageError::InsertError(e))
        }
    }
}
```

**The `?` operator:**
```rust
// This:
let settings = config_manager.load_settings(key).await?;

// Is shorthand for:
let settings = match config_manager.load_settings(key).await {
    Ok(s) => s,
    Err(e) => return Err(e.into()),
};
```

---

## Traits (Interfaces)

Traits define shared behavior:

```rust
// From src/metrics/mod.rs

#[async_trait]
pub trait MetricCollector: Send + Sync {
    fn name(&self) -> &str;

    async fn collect(
        &self,
        node_id: &str
    ) -> Result<Document, Box<dyn Error + Send + Sync>>;
}
```

**Trait Bounds:**
- `Send + Sync`: Can be safely sent between threads
- Required for async tasks

### Implementing Traits

```rust
// From src/metrics/load_average.rs

pub struct LoadAverageCollector {
    system: System,
}

impl LoadAverageCollector {
    pub fn new() -> Self {
        LoadAverageCollector {
            system: System::new(),
        }
    }
}

// Implement the trait
#[async_trait]
impl MetricCollector for LoadAverageCollector {
    fn name(&self) -> &str {
        "LoadAverage"
    }

    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        let load_avg = System::load_average();
        let doc = doc! {
            "node": node_id,
            "load_1min": load_avg.one,
        };
        Ok(doc)
    }
}
```

### Trait Objects (Dynamic Dispatch)

```rust
// From src/metrics/mod.rs

// Box<dyn MetricCollector> is a trait object
// "dyn" means dynamic dispatch (runtime polymorphism)
pub fn create_all_collectors() -> Vec<Box<dyn MetricCollector>> {
    vec![
        Box::new(LoadAverageCollector::new()),
        Box::new(MemoryCollector::new()),
        Box::new(DiskCollector::new()),
        Box::new(DockerCollector::new()),
    ]
}
```

**Why trait objects?**
- Store different types in the same collection
- Call methods without knowing exact type at compile time
- Similar to interfaces in Java/C#

---

## Error Handling

Rust doesn't have exceptions - it uses `Result` type:

### Result Type

```rust
// From src/storage.rs

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("MongoDB insert failed: {0}")]
    InsertError(#[from] mongodb::error::Error),

    #[error("Invalid document format: {0}")]
    InvalidDocument(String),
}

pub async fn store_metric(
    &self,
    collection_name: &str,
    document: Document,
) -> Result<(), StorageError> {
    let collection: Collection<Document> = db.collection(collection_name);

    match collection.insert_one(document, None).await {
        Ok(result) => {
            debug!("Stored with id: {:?}", result.inserted_id);
            Ok(())
        }
        Err(e) => {
            error!("Storage failed: {}", e);
            Err(StorageError::InsertError(e))
        }
    }
}
```

### The `?` Operator

```rust
// From src/config.rs

pub async fn new(
    connection_string: &str,
    database_name: Option<&str>,
) -> Result<Self, ConfigError> {
    // If this fails, function returns early with the error
    let client = Client::with_uri_str(connection_string).await?;

    // ? converts the error type if needed (using From trait)
    match client.list_database_names(None, None).await {
        Ok(_) => info!("Connected successfully"),
        Err(e) => return Err(ConfigError::MongoConnectionError(e)),
    }

    Ok(ConfigManager {
        client,
        database_name: database_name.unwrap_or("monitoring").to_string(),
    })
}
```

### Using `anyhow` for Application Errors

```rust
// From src/main.rs

use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    // .context() adds context to errors for better debugging
    let args = parse_arguments()
        .context("Failed to parse arguments")?;

    let config_manager = ConfigManager::new(&args.mongodb_uri, Some(&args.database_name))
        .await
        .context("Failed to connect to MongoDB")?;

    Ok(())
}
```

**Error handling patterns:**
- Use `Result` for recoverable errors
- Use `panic!` only for unrecoverable errors
- Add context with `.context()` for better error messages

---

## Async/Await

Rust's async/await enables efficient concurrent programming:

### Async Functions

```rust
// From src/metrics/load_average.rs

#[async_trait]
impl MetricCollector for LoadAverageCollector {
    // async functions return a Future
    async fn collect(&self, node_id: &str) -> Result<Document, Box<dyn Error + Send + Sync>> {
        debug!("Collecting load average metrics");

        // This is not async, but the function signature requires async
        let load_avg = System::load_average();

        let doc = doc! {
            "node": node_id,
            "timestamp": Utc::now(),
            "load_1min": load_avg.one,
        };

        Ok(doc)
    }
}
```

### Awaiting Futures

```rust
// From src/scheduler.rs

pub async fn run_metric_task(...) {
    let mut interval_timer = interval(Duration::from_secs(interval_secs));

    loop {
        // .await suspends execution until the future completes
        interval_timer.tick().await;

        // collect() is async, so we must await it
        match collector.collect(&node_id).await {
            Ok(document) => {
                // store_metric_safe is async too
                storage.store_metric_safe(&collection_name, metric_name, document).await;
            }
            Err(e) => {
                error!("Collection failed: {}", e);
            }
        }
    }
}
```

### Spawning Tasks

```rust
// From src/scheduler.rs

pub async fn start(self, collectors: Vec<Box<dyn MetricCollector>>) {
    let mut handles = Vec::new();

    for collector in collectors {
        let storage = Arc::clone(&self.storage);
        let node_id = self.node_id.clone();

        // Spawn a new async task (runs concurrently)
        let handle = tokio::spawn(async move {
            // This runs in parallel with other tasks
            Self::run_metric_task(collector, storage, node_id, timeout, collection).await;
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
}
```

**Key Points:**
- `async fn` returns a `Future`
- `Future` does nothing until `.await`ed
- `tokio::spawn` creates a new task
- Tasks run concurrently on the Tokio runtime

### The Tokio Runtime

```rust
// From src/main.rs

// #[tokio::main] creates the async runtime
#[tokio::main]
async fn main() -> Result<()> {
    // All async code runs on this runtime
    let config = ConfigManager::new("...").await?;
    scheduler.start(collectors).await;
    Ok(())
}

// This expands to:
fn main() -> Result<()> {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            let config = ConfigManager::new("...").await?;
            scheduler.start(collectors).await;
            Ok(())
        })
}
```

---

## Modules and Organization

### Module Structure

```rust
// From src/main.rs

// Declare modules
mod config;        // Looks for src/config.rs
mod metrics;       // Looks for src/metrics/mod.rs or src/metrics.rs
mod scheduler;
mod storage;

// Use items from modules
use config::ConfigManager;
use metrics::create_all_collectors;
use scheduler::MetricScheduler;
```

### Module Files

```
src/
├── main.rs
├── config.rs       → mod config;
├── metrics/
│   ├── mod.rs      → mod metrics;
│   ├── disk.rs     → pub mod disk; (in metrics/mod.rs)
│   └── memory.rs   → pub mod memory; (in metrics/mod.rs)
```

### Re-exporting

```rust
// From src/metrics/mod.rs

// Declare submodules (private by default)
pub mod load_average;
pub mod memory;
pub mod disk;
pub mod docker;

// Re-export the trait for convenience
pub use load_average::LoadAverageCollector;

// Now users can do:
// use metrics::LoadAverageCollector;
// Instead of:
// use metrics::load_average::LoadAverageCollector;
```

### Visibility

```rust
// No keyword = private (only in same module)
fn private_function() {}

// pub = public (accessible from anywhere)
pub fn public_function() {}

// pub(crate) = public within this crate only
pub(crate) fn crate_only_function() {}

// pub(super) = public in parent module
pub(super) fn parent_only_function() {}
```

---

## Common Patterns in This Project

### 1. Builder Pattern with `new()`

```rust
pub struct LoadAverageCollector {
    system: System,
}

impl LoadAverageCollector {
    pub fn new() -> Self {
        LoadAverageCollector {
            system: System::new(),
        }
    }
}

// Usage:
let collector = LoadAverageCollector::new();
```

### 2. Type Aliases for Clarity

```rust
use std::error::Error;

// Instead of writing this everywhere:
Result<Document, Box<dyn Error + Send + Sync>>

// You could define:
type MetricResult = Result<Document, Box<dyn Error + Send + Sync>>;

// Then use:
async fn collect(&self, node_id: &str) -> MetricResult {
    // ...
}
```

### 3. The `doc!` Macro

```rust
// From src/metrics/load_average.rs

use bson::doc;

let doc = doc! {
    "node": node_id,
    "timestamp": Utc::now(),
    "load_1min": load_avg.one,
    "cpu_cores": cpu_count as i32,
};
```

**Macro basics:**
- Macros are denoted by `!`
- They generate code at compile time
- `doc!` creates BSON documents

### 4. Derive Macros

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringSettings {
    pub key: String,
    pub metric_settings: HashMap<String, MetricSettings>,
}
```

**Common derives:**
- `Debug` - Enables formatting with `{:?}`
- `Clone` - Enables `.clone()`
- `Serialize/Deserialize` - For JSON/BSON conversion
- `Default` - Provides default values
- `Error` - Implements Error trait (from thiserror)

### 5. Logging with `tracing`

```rust
use tracing::{debug, info, warn, error};

info!("Starting application");
debug!("Collecting metric: {}", metric_name);
warn!("Docker unavailable: {}", error);
error!("Failed to connect: {}", e);

// With structured fields:
info!(
    metric = metric_name,
    interval = timeout,
    "Scheduling metric"
);
```

### 6. String Types

```rust
// &str - borrowed string slice (doesn't own the data)
fn process_name(name: &str) {
    println!("{}", name);
}

// String - owned, growable string
fn create_message() -> String {
    String::from("Hello, world!")
}

// Converting between them:
let owned: String = String::from("test");
let borrowed: &str = &owned;
let borrowed2: &str = "literal";
let owned2: String = borrowed2.to_string();
```

### 7. Collections

```rust
use std::collections::HashMap;

// Vector (dynamic array)
let mut collectors: Vec<Box<dyn MetricCollector>> = Vec::new();
collectors.push(Box::new(LoadAverageCollector::new()));

// HashMap (key-value store)
let mut settings: HashMap<String, MetricSettings> = HashMap::new();
settings.insert("LoadAverage".to_string(), MetricSettings {
    timeout: 5,
    collection: "load_average_metrics".to_string(),
});

// Access
if let Some(setting) = settings.get("LoadAverage") {
    println!("Timeout: {}", setting.timeout);
}
```

### 8. Iterators

```rust
// From src/metrics/disk.rs

let disks = Disks::new_with_refreshed_list();

// Iterate over disks
for disk in disks.list() {
    let mount_point = disk.mount_point().to_string_lossy().to_string();
    println!("{}", mount_point);
}

// Iterator methods:
let total: u64 = disks.list()
    .map(|disk| disk.total_space())
    .sum();

let mount_points: Vec<String> = disks.list()
    .map(|disk| disk.mount_point().to_string_lossy().to_string())
    .collect();
```

---

## Learning Resources

### Official Resources

- **The Rust Book**: https://doc.rust-lang.org/book/
- **Rust by Example**: https://doc.rust-lang.org/rust-by-example/
- **Standard Library Docs**: https://doc.rust-lang.org/std/

### Topics to Explore Next

1. **Lifetimes** - Advanced borrowing
2. **Generics** - Code that works with multiple types
3. **Macros** - Metaprogramming
4. **Testing** - Unit and integration tests
5. **Cargo** - Rust's build tool and package manager

### Project-Specific Learning

To understand this project better:

1. Start with `src/main.rs` - See how everything connects
2. Read `src/metrics/load_average.rs` - Simplest metric
3. Read `src/config.rs` - See async and error handling
4. Read `src/scheduler.rs` - See async tasks and Arc
5. Try adding a new metric using `docs/adding-new-metrics.md`

---

## Common Beginner Questions

### Why does Rust have both `String` and `&str`?

- `String` - Owned, growable, heap-allocated
- `&str` - Borrowed, fixed-size, can point to literal or String

```rust
let owned = String::from("hello");  // Heap allocated
let borrowed = "hello";              // String literal
let slice = &owned[0..2];           // Slice of owned
```

### What's the difference between `clone()` and `copy()`?

- `Copy` - Implicit, cheap (integers, bools, etc.)
- `Clone` - Explicit, potentially expensive

```rust
let x = 5;
let y = x;  // x is Copied (still valid)

let s1 = String::from("hello");
let s2 = s1;  // s1 is moved (no longer valid)
let s3 = s2.clone();  // s2 is cloned (both valid)
```

### When to use `&` vs `&mut` vs no reference?

```rust
// Read-only access (most common)
fn read_value(x: &String) {
    println!("{}", x);
}

// Modify in place
fn modify_value(x: &mut String) {
    x.push_str(" world");
}

// Take ownership (consume)
fn consume_value(x: String) {
    // x is dropped at end of function
}
```

### What does `Box<dyn Trait>` mean?

- `Box` - Heap allocation
- `dyn` - Dynamic dispatch (runtime polymorphism)
- `Trait` - The trait being used

```rust
// Store different types implementing same trait
let collectors: Vec<Box<dyn MetricCollector>> = vec![
    Box::new(LoadAverageCollector::new()),
    Box::new(MemoryCollector::new()),
];
```

---

## Conclusion

This guide covered the essential Rust concepts used in this project:

✓ Ownership and borrowing
✓ Structs and implementations
✓ Enums and pattern matching
✓ Traits (interfaces)
✓ Error handling with Result
✓ Async/await concurrency
✓ Modules and organization

**Next Steps:**
1. Read through the project code
2. Try modifying existing metrics
3. Add a new metric (follow `docs/adding-new-metrics.md`)
4. Experiment with the examples in this guide

Remember: Rust has a steep learning curve, but the compiler is your friend - it will guide you to correct code!
