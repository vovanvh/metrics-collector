# Rust Cheatsheet

Quick reference for Rust syntax and common patterns, with examples from the Metrics Collector project.

## Table of Contents

1. [Variables & Types](#variables--types)
2. [Functions](#functions)
3. [Control Flow](#control-flow)
4. [Ownership & Borrowing](#ownership--borrowing)
5. [Structs & Enums](#structs--enums)
6. [Traits](#traits)
7. [Error Handling](#error-handling)
8. [Collections](#collections)
9. [Iterators](#iterators)
10. [Strings](#strings)
11. [Async/Await](#asyncawait)
12. [Macros](#macros)
13. [Common Patterns](#common-patterns)

---

## Variables & Types

### Variable Declaration

```rust
let x = 5;                    // Immutable (default)
let mut y = 10;               // Mutable
const MAX: i32 = 100;         // Constant (must be typed)
static NAME: &str = "app";    // Static variable
```

### Type Annotations

```rust
let x: i32 = 5;               // Explicit type
let y = 5i32;                 // Type suffix
let z: f64 = 3.14;            // Float
let s: &str = "hello";        // String slice
let owned: String = String::from("test");  // Owned string
```

### Primitive Types

```rust
// Integers
i8, i16, i32, i64, i128, isize       // Signed
u8, u16, u32, u64, u128, usize       // Unsigned

// Floats
f32, f64

// Boolean
let b: bool = true;

// Character
let c: char = 'z';

// Unit (like void)
let u: () = ();
```

### Type Casting

```rust
let x: i32 = 5;
let y: i64 = x as i64;        // Explicit cast
let f: f64 = x as f64;

// Example from project:
let cpu_count = num_cpus::get();
let cores: i32 = cpu_count as i32;
```

---

## Functions

### Basic Function

```rust
fn add(x: i32, y: i32) -> i32 {
    x + y  // No semicolon = return value
}

fn print_message(msg: &str) {
    println!("{}", msg);  // No return type = ()
}
```

### Multiple Return Values

```rust
fn divide(x: i32, y: i32) -> Result<i32, String> {
    if y == 0 {
        Err(String::from("Division by zero"))
    } else {
        Ok(x / y)
    }
}
```

### Generic Functions

```rust
fn largest<T: PartialOrd>(list: &[T]) -> &T {
    let mut largest = &list[0];
    for item in list {
        if item > largest {
            largest = item;
        }
    }
    largest
}
```

---

## Control Flow

### If/Else

```rust
if x > 5 {
    println!("Greater");
} else if x == 5 {
    println!("Equal");
} else {
    println!("Less");
}

// If as expression
let y = if x > 0 { 1 } else { -1 };
```

### Match

```rust
match value {
    1 => println!("One"),
    2 | 3 => println!("Two or Three"),
    4..=10 => println!("Four through Ten"),
    _ => println!("Something else"),
}

// Match with Result
match result {
    Ok(value) => println!("Success: {}", value),
    Err(e) => eprintln!("Error: {}", e),
}

// Match with Option
match option {
    Some(value) => println!("Got: {}", value),
    None => println!("Nothing"),
}
```

### If Let

```rust
// Instead of:
match option {
    Some(value) => println!("{}", value),
    None => {},
}

// Use:
if let Some(value) = option {
    println!("{}", value);
}
```

### Loops

```rust
// Infinite loop
loop {
    break;  // Exit loop
}

// While loop
while condition {
    // ...
}

// For loop
for i in 0..10 {  // 0 to 9
    println!("{}", i);
}

for i in 0..=10 {  // 0 to 10 (inclusive)
    println!("{}", i);
}

for item in collection.iter() {
    println!("{}", item);
}
```

---

## Ownership & Borrowing

### Ownership Rules

```rust
// 1. Each value has one owner
let s1 = String::from("hello");

// 2. Transfer ownership (move)
let s2 = s1;  // s1 is no longer valid

// 3. Value is dropped when owner goes out of scope
{
    let s3 = String::from("test");
}  // s3 is dropped here
```

### Borrowing

```rust
// Immutable borrow
fn read(s: &String) {
    println!("{}", s);
}
let s = String::from("hello");
read(&s);  // s is still valid

// Mutable borrow
fn modify(s: &mut String) {
    s.push_str(" world");
}
let mut s = String::from("hello");
modify(&mut s);

// Rules:
// - Multiple immutable borrows OK
// - One mutable borrow at a time
// - Can't have mutable and immutable simultaneously
```

### Clone vs Copy

```rust
// Copy (implicit, for small types)
let x = 5;
let y = x;  // x still valid

// Clone (explicit, for heap types)
let s1 = String::from("hello");
let s2 = s1.clone();  // Both valid
```

### Arc (Atomic Reference Counting)

```rust
use std::sync::Arc;

let data = Arc::new(vec![1, 2, 3]);
let data2 = Arc::clone(&data);  // Cheap: just increments counter

// Use in async tasks
tokio::spawn(async move {
    println!("{:?}", data2);
});
```

---

## Structs & Enums

### Structs

```rust
// Define struct
struct User {
    username: String,
    email: String,
    active: bool,
}

// Create instance
let user = User {
    username: String::from("john"),
    email: String::from("john@example.com"),
    active: true,
};

// Tuple struct
struct Point(i32, i32);
let p = Point(10, 20);

// Unit struct
struct Marker;
```

### Implementation

```rust
impl User {
    // Associated function (static method)
    fn new(username: String, email: String) -> Self {
        Self {
            username,
            email,
            active: true,
        }
    }

    // Method
    fn deactivate(&mut self) {
        self.active = false;
    }

    // Method with reference
    fn is_active(&self) -> bool {
        self.active
    }
}

// Usage
let mut user = User::new(
    String::from("john"),
    String::from("john@example.com")
);
user.deactivate();
```

### Enums

```rust
// Simple enum
enum Status {
    Active,
    Inactive,
    Pending,
}

// Enum with data
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(i32, i32, i32),
}

// Pattern match on enum
match msg {
    Message::Quit => println!("Quit"),
    Message::Move { x, y } => println!("Move to {}, {}", x, y),
    Message::Write(text) => println!("Write: {}", text),
    Message::ChangeColor(r, g, b) => println!("Color: {}, {}, {}", r, g, b),
}
```

### Option & Result

```rust
// Option - value or nothing
enum Option<T> {
    Some(T),
    None,
}

let some_num: Option<i32> = Some(5);
let no_num: Option<i32> = None;

// Methods
some_num.unwrap();           // Panics if None
some_num.unwrap_or(0);      // Default value
some_num.unwrap_or_else(|| 0);
some_num.expect("No value"); // Panic with message

// Result - success or error
enum Result<T, E> {
    Ok(T),
    Err(E),
}

let result: Result<i32, String> = Ok(5);

// Methods
result.unwrap();
result.expect("Failed");
result.unwrap_or(0);
result.is_ok();
result.is_err();
```

---

## Traits

### Defining Traits

```rust
trait Summary {
    fn summarize(&self) -> String;

    // Default implementation
    fn print_summary(&self) {
        println!("{}", self.summarize());
    }
}
```

### Implementing Traits

```rust
struct Article {
    title: String,
    content: String,
}

impl Summary for Article {
    fn summarize(&self) -> String {
        format!("{}: {}...", self.title, &self.content[..20])
    }
}
```

### Trait Bounds

```rust
// Generic with trait bound
fn notify<T: Summary>(item: &T) {
    println!("{}", item.summarize());
}

// Multiple trait bounds
fn notify<T: Summary + Display>(item: &T) {
    // ...
}

// Where clause
fn notify<T>(item: &T)
where
    T: Summary + Display,
{
    // ...
}
```

### Common Traits

```rust
#[derive(Debug)]        // {:?} formatting
#[derive(Clone)]        // .clone() method
#[derive(Copy)]         // Implicit copying
#[derive(PartialEq)]    // == and != operators
#[derive(Eq)]           // Full equality
#[derive(PartialOrd)]   // <, >, <=, >=
#[derive(Ord)]          // Full ordering
#[derive(Hash)]         // Hashing for HashMap
#[derive(Default)]      // Default values

// Example:
#[derive(Debug, Clone, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}
```

---

## Error Handling

### Result Type

```rust
fn divide(x: i32, y: i32) -> Result<i32, String> {
    if y == 0 {
        Err(String::from("Division by zero"))
    } else {
        Ok(x / y)
    }
}

// Usage
match divide(10, 2) {
    Ok(result) => println!("Result: {}", result),
    Err(e) => eprintln!("Error: {}", e),
}
```

### The `?` Operator

```rust
fn read_file() -> Result<String, std::io::Error> {
    let content = std::fs::read_to_string("file.txt")?;
    Ok(content)
}

// Equivalent to:
fn read_file() -> Result<String, std::io::Error> {
    let content = match std::fs::read_to_string("file.txt") {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    Ok(content)
}
```

### Custom Errors with thiserror

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),
}
```

### Using anyhow for Applications

```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let config = load_config()
        .context("Failed to load config")?;

    let db = connect_db(&config.db_url)
        .context("Failed to connect to database")?;

    Ok(())
}
```

---

## Collections

### Vec (Vector)

```rust
// Create
let mut v: Vec<i32> = Vec::new();
let v = vec![1, 2, 3];
let v = vec![0; 5];  // [0, 0, 0, 0, 0]

// Add elements
v.push(4);
v.extend([5, 6, 7]);

// Access
let first = &v[0];
let first = v.get(0);  // Returns Option<&T>

// Remove
v.pop();           // Remove last
v.remove(0);       // Remove at index

// Iterate
for item in &v {
    println!("{}", item);
}
```

### HashMap

```rust
use std::collections::HashMap;

// Create
let mut map = HashMap::new();
map.insert(String::from("Blue"), 10);
map.insert(String::from("Red"), 20);

// Access
let value = map.get("Blue");  // Option<&V>

// Update
map.insert(String::from("Blue"), 25);

// Entry API
map.entry(String::from("Green")).or_insert(30);

// Iterate
for (key, value) in &map {
    println!("{}: {}", key, value);
}
```

### HashSet

```rust
use std::collections::HashSet;

let mut set = HashSet::new();
set.insert(1);
set.insert(2);

if set.contains(&1) {
    println!("Has 1");
}
```

---

## Iterators

### Common Iterator Methods

```rust
let v = vec![1, 2, 3, 4, 5];

// map - transform each element
let doubled: Vec<i32> = v.iter()
    .map(|x| x * 2)
    .collect();

// filter - keep matching elements
let even: Vec<i32> = v.iter()
    .filter(|x| *x % 2 == 0)
    .copied()
    .collect();

// fold - reduce to single value
let sum: i32 = v.iter().fold(0, |acc, x| acc + x);

// sum (specialized fold)
let sum: i32 = v.iter().sum();

// collect - build collection from iterator
let v2: Vec<i32> = v.iter().copied().collect();

// enumerate - get index
for (i, value) in v.iter().enumerate() {
    println!("{}: {}", i, value);
}

// zip - combine two iterators
let names = vec!["Alice", "Bob"];
let ages = vec![20, 30];
for (name, age) in names.iter().zip(ages.iter()) {
    println!("{} is {}", name, age);
}
```

### Iterator Adapters

```rust
// chain - combine iterators
let v1 = vec![1, 2];
let v2 = vec![3, 4];
let combined: Vec<i32> = v1.iter()
    .chain(v2.iter())
    .copied()
    .collect();

// take - first N elements
let first_three: Vec<i32> = v.iter()
    .take(3)
    .copied()
    .collect();

// skip - skip first N
let skip_two: Vec<i32> = v.iter()
    .skip(2)
    .copied()
    .collect();

// find - first matching element
let found = v.iter().find(|x| **x > 3);  // Option<&i32>

// any - check if any matches
let has_even = v.iter().any(|x| x % 2 == 0);

// all - check if all match
let all_positive = v.iter().all(|x| *x > 0);
```

---

## Strings

### String Types

```rust
// &str - borrowed string slice
let s: &str = "hello";

// String - owned, growable
let s: String = String::from("hello");
let s: String = "hello".to_string();
```

### String Operations

```rust
let mut s = String::from("hello");

// Append
s.push_str(" world");
s.push('!');

// Concatenate
let s1 = String::from("Hello");
let s2 = String::from(" world");
let s3 = s1 + &s2;  // s1 is moved

// Format macro
let s = format!("{} {}", "Hello", "world");

// Length
let len = s.len();

// Slice
let slice = &s[0..5];

// Contains
if s.contains("world") {
    println!("Found!");
}

// Replace
let new_s = s.replace("world", "Rust");

// Split
for word in s.split_whitespace() {
    println!("{}", word);
}
```

### String Conversion

```rust
// &str -> String
let s: String = "hello".to_string();
let s: String = String::from("hello");

// String -> &str
let s: String = String::from("hello");
let slice: &str = &s;
let slice: &str = s.as_str();

// Number -> String
let num = 42;
let s = num.to_string();
let s = format!("{}", num);

// String -> Number
let s = "42";
let num: i32 = s.parse().unwrap();
let num: Result<i32, _> = s.parse();
```

---

## Async/Await

### Async Functions

```rust
// Async function
async fn fetch_data() -> Result<String, Error> {
    let response = reqwest::get("https://api.example.com")
        .await?;
    let text = response.text().await?;
    Ok(text)
}

// Calling async function
let data = fetch_data().await?;
```

### Tokio Runtime

```rust
// Main function with Tokio
#[tokio::main]
async fn main() {
    let result = fetch_data().await;
}

// Manual runtime
fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let result = fetch_data().await;
    });
}
```

### Spawning Tasks

```rust
// Spawn concurrent task
let handle = tokio::spawn(async {
    println!("Hello from task");
});

// Wait for task
handle.await.unwrap();

// Spawn multiple tasks
let mut handles = vec![];
for i in 0..10 {
    let handle = tokio::spawn(async move {
        println!("Task {}", i);
    });
    handles.push(handle);
}

// Wait for all
for handle in handles {
    handle.await.unwrap();
}
```

### Async Traits

```rust
use async_trait::async_trait;

#[async_trait]
trait Repository {
    async fn save(&self, data: &str) -> Result<(), Error>;
}

#[async_trait]
impl Repository for MyRepo {
    async fn save(&self, data: &str) -> Result<(), Error> {
        // Implementation
        Ok(())
    }
}
```

### Sleep and Intervals

```rust
use tokio::time::{sleep, interval, Duration};

// Sleep
sleep(Duration::from_secs(1)).await;

// Interval
let mut interval = interval(Duration::from_secs(5));
loop {
    interval.tick().await;
    println!("5 seconds elapsed");
}
```

---

## Macros

### Common Macros

```rust
// Print
println!("Hello, {}!", name);
eprintln!("Error: {}", err);

// Format
let s = format!("x = {}, y = {}", x, y);

// Debug print
dbg!(value);
dbg!(&value);

// Assert
assert!(condition);
assert_eq!(left, right);
assert_ne!(left, right);

// Panic
panic!("Something went wrong!");

// Vector
let v = vec![1, 2, 3];

// Include files
let config = include_str!("config.toml");
let bytes = include_bytes!("data.bin");
```

### Macro from This Project

```rust
// BSON document creation
use bson::doc;

let document = doc! {
    "name": "John",
    "age": 30,
    "active": true,
};
```

---

## Common Patterns

### Constructor Pattern

```rust
impl MyStruct {
    pub fn new() -> Self {
        Self {
            field1: default_value(),
            field2: another_value(),
        }
    }
}
```

### Builder Pattern

```rust
pub struct Builder {
    field1: Option<String>,
    field2: Option<i32>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            field1: None,
            field2: None,
        }
    }

    pub fn field1(mut self, value: String) -> Self {
        self.field1 = Some(value);
        self
    }

    pub fn field2(mut self, value: i32) -> Self {
        self.field2 = Some(value);
        self
    }

    pub fn build(self) -> Result<MyStruct, Error> {
        Ok(MyStruct {
            field1: self.field1.ok_or("field1 required")?,
            field2: self.field2.unwrap_or(0),
        })
    }
}

// Usage
let obj = Builder::new()
    .field1("value".to_string())
    .field2(42)
    .build()?;
```

### RAII (Resource Acquisition Is Initialization)

```rust
// File is automatically closed when 'file' goes out of scope
{
    let file = File::open("data.txt")?;
    // Use file
}  // File closed here

// Same with locks
{
    let guard = mutex.lock().unwrap();
    // Critical section
}  // Lock released here
```

### Newtype Pattern

```rust
// Wrap type for type safety
struct UserId(i32);
struct ProductId(i32);

// Can't accidentally mix them
fn get_user(id: UserId) { }
fn get_product(id: ProductId) { }

let user_id = UserId(1);
let product_id = ProductId(1);

get_user(user_id);        // OK
get_user(product_id);     // Compile error!
```

---

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addition() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_division() {
        assert_eq!(divide(10, 2), Ok(5));
        assert!(divide(10, 0).is_err());
    }

    #[test]
    #[should_panic]
    fn test_panic() {
        panic!("This should panic");
    }

    #[tokio::test]
    async fn test_async() {
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

```rust
// In tests/ directory
// tests/integration_test.rs

use my_crate::*;

#[test]
fn test_integration() {
    let result = public_function();
    assert_eq!(result, expected);
}
```

---

## Cargo Commands

```bash
# Create new project
cargo new my_project
cargo new --lib my_library

# Build
cargo build              # Debug build
cargo build --release    # Optimized build

# Run
cargo run               # Build and run
cargo run --release     # Run release build

# Test
cargo test              # Run all tests
cargo test test_name    # Run specific test
cargo test -- --nocapture  # Show println! output

# Check (fast compile check)
cargo check

# Documentation
cargo doc               # Build docs
cargo doc --open        # Build and open docs

# Format
cargo fmt               # Format code

# Lint
cargo clippy            # Run linter

# Update dependencies
cargo update

# Clean build artifacts
cargo clean

# Add dependency
cargo add serde
```

---

## Quick Tips

### Debug Print

```rust
// Implement Debug trait
#[derive(Debug)]
struct Point { x: i32, y: i32 }

let p = Point { x: 10, y: 20 };
println!("{:?}", p);      // Point { x: 10, y: 20 }
println!("{:#?}", p);     // Pretty print
dbg!(p);                  // Debug with file/line
```

### Conditional Compilation

```rust
#[cfg(test)]
mod tests { }

#[cfg(debug_assertions)]
println!("Debug mode");

#[cfg(target_os = "linux")]
fn linux_only() { }
```

### Documentation Comments

```rust
/// This function adds two numbers
///
/// # Examples
///
/// ```
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Common Compiler Messages

```rust
// "cannot borrow as mutable"
let x = 5;
x = 6;  // Error: use 'let mut x'

// "value used after move"
let s1 = String::from("hello");
let s2 = s1;
println!("{}", s1);  // Error: use s1.clone()

// "cannot borrow as mutable more than once"
let mut v = vec![1, 2, 3];
let r1 = &mut v;
let r2 = &mut v;  // Error: only one &mut allowed

// "lifetime error"
// Usually need to add lifetime annotations
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

---

## Resources

- **Official Docs**: https://doc.rust-lang.org/
- **Rust Book**: https://doc.rust-lang.org/book/
- **Rust by Example**: https://doc.rust-lang.org/rust-by-example/
- **Std Lib Docs**: https://doc.rust-lang.org/std/
- **Crates.io**: https://crates.io/ (package registry)
- **Docs.rs**: https://docs.rs/ (crate documentation)

---

## This Project's Patterns

### Main Pattern

```rust
#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let args = parse_arguments()?;
    let config = ConfigManager::new(&args.mongodb_uri, Some(&args.database_name)).await?;
    let settings = config.load_settings(&args.config_key).await?;
    Ok(())
}
```

### Trait Object Pattern

```rust
let collectors: Vec<Box<dyn MetricCollector>> = vec![
    Box::new(LoadAverageCollector::new()),
    Box::new(MemoryCollector::new()),
];
```

### Arc Pattern for Sharing

```rust
let settings = Arc::new(settings);
let storage = Arc::new(storage);

for collector in collectors {
    let settings = Arc::clone(&settings);
    tokio::spawn(async move { /* use settings */ });
}
```

### Error Handling Pattern

```rust
async fn collect(&self) -> Result<Document, Box<dyn Error + Send + Sync>> {
    let data = fetch_data()?;
    let doc = doc! { "data": data };
    Ok(doc)
}
```

---

**Happy Coding in Rust!** 🦀
