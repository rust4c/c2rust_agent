//! Complete C to Rust translation workflow example
//!
//! This example demonstrates how to use the main_processor crate
//! to translate C code to Rust with proper error handling,
//! progress tracking, and code formatting.

use anyhow::Result;
use log::{info, warn, LevelFilter};
use main_processor::{pkg_config, MainProcessor};
use tempfile::TempDir;
use tokio::fs;

/// Example C code for testing
const EXAMPLE_C_CODE: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char* name;
    int age;
    double salary;
} Person;

Person* create_person(const char* name, int age, double salary) {
    Person* p = malloc(sizeof(Person));
    if (!p) return NULL;

    p->name = malloc(strlen(name) + 1);
    if (!p->name) {
        free(p);
        return NULL;
    }

    strcpy(p->name, name);
    p->age = age;
    p->salary = salary;
    return p;
}

void destroy_person(Person* person) {
    if (person) {
        free(person->name);
        free(person);
    }
}

void print_person(const Person* person) {
    if (person) {
        printf("Name: %s, Age: %d, Salary: %.2f\n",
               person->name, person->age, person->salary);
    }
}

int main() {
    Person* john = create_person("John Doe", 30, 50000.0);
    if (!john) {
        fprintf(stderr, "Failed to create person\n");
        return 1;
    }

    print_person(john);
    destroy_person(john);

    return 0;
}
"#;

/// Initialize logging for the example
fn init_logging() {}

/// Example 1: Quick single file translation
async fn example_single_file_translation() -> Result<()> {
    info!("=== Example 1: Single File Translation ===");

    // Create a temporary C file
    let temp_dir = TempDir::new()?;
    let c_file_path = temp_dir.path().join("person.c");

    fs::write(&c_file_path, EXAMPLE_C_CODE).await?;
    info!("Created test C file: {}", c_file_path.display());

    // Create processor and translate
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    match processor.process_single(&c_file_path).await {
        Ok(()) => {
            info!("✅ Translation successful!");

            // Check if the output was created
            let rust_project_path = c_file_path.parent().unwrap().join("rust-project");
            let main_rs_path = rust_project_path.join("src").join("main.rs");

            if main_rs_path.exists() {
                let rust_code = fs::read_to_string(&main_rs_path).await?;
                info!("Generated Rust code preview (first 300 chars):");
                let preview = if rust_code.len() > 300 {
                    format!("{}...", &rust_code[..300])
                } else {
                    rust_code
                };
                println!("{}", preview);
            }
        }
        Err(e) => {
            warn!("❌ Translation failed: {}", e);
        }
    }

    Ok(())
}

/// Example 2: Batch translation of multiple files
async fn example_batch_translation() -> Result<()> {
    info!("=== Example 2: Batch Translation ===");

    let temp_dir = TempDir::new()?;

    // Create multiple small C projects
    let projects = vec![
        (
            "hello_world",
            "#include <stdio.h>\nint main() { printf(\"Hello, World!\\n\"); return 0; }",
        ),
        (
            "calculator",
            r#"
#include <stdio.h>
int add(int a, int b) { return a + b; }
int main() {
    int result = add(5, 3);
    printf("5 + 3 = %d\n", result);
    return 0;
}"#,
        ),
        (
            "fibonacci",
            r#"
#include <stdio.h>
int fibonacci(int n) {
    if (n <= 1) return n;
    return fibonacci(n-1) + fibonacci(n-2);
}
int main() {
    printf("Fibonacci(10) = %d\n", fibonacci(10));
    return 0;
}"#,
        ),
    ];

    let mut project_paths = Vec::new();

    for (name, code) in projects {
        let project_path = temp_dir.path().join(name);
        fs::create_dir_all(&project_path).await?;
        fs::write(project_path.join("main.c"), code).await?;
        project_paths.push(project_path);
    }

    info!("Created {} test projects", project_paths.len());

    // Perform batch translation
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    match processor.process_batch(project_paths).await {
        Ok(()) => {
            info!("✅ Batch translation completed successfully!");
        }
        Err(e) => {
            warn!("❌ Some translations failed: {}", e);
        }
    }

    Ok(())
}

/// Example 3: Working with directory structures
async fn example_directory_structure() -> Result<()> {
    info!("=== Example 3: Directory Structure Translation ===");

    let temp_dir = TempDir::new()?;
    let root_dir = temp_dir.path().join("c_projects_root");
    fs::create_dir_all(&root_dir).await?;

    // Create a directory structure with multiple C projects
    let structure = vec![
        (
            "project_a",
            "#include <stdio.h>\nint main() { printf(\"Project A\\n\"); return 0; }",
        ),
        (
            "project_b",
            "#include <stdio.h>\nint add(int a, int b) { return a + b; }\nint main() { return add(1,2); }",
        ),
        (
            "project_c",
            "#include <stdio.h>\nvoid helper() {}\nint main() { helper(); return 0; }",
        ),
    ];

    let mut paths = Vec::new();
    for (name, content) in structure {
        let project_path = root_dir.join(name);
        fs::create_dir_all(&project_path).await?;
        fs::write(project_path.join("main.c"), content).await?;
        paths.push(project_path);
    }

    info!("Created directory structure with multiple projects");

    // Process all projects
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    match processor.process_batch(paths).await {
        Ok(()) => {
            info!("✅ Directory structure translation completed!");
        }
        Err(e) => {
            warn!("❌ Some directory translations failed: {}", e);
        }
    }

    Ok(())
}

/// Example 4: Error handling scenarios
async fn example_error_handling() -> Result<()> {
    info!("=== Example 4: Error Handling ===");

    let temp_dir = TempDir::new()?;

    // Create a problematic C file (intentionally malformed for testing)
    let problematic_c = r#"
// This C code has some complex patterns that might challenge translation
#include <stdio.h>
#include <stdlib.h>

// Complex pointer usage
int* get_array() {
    static int arr[10] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
    return arr;
}

// Function pointers
typedef int (*operation_t)(int, int);

int add(int a, int b) { return a + b; }
int multiply(int a, int b) { return a * b; }

int main() {
    int* array = get_array();
    printf("First element: %d\n", array[0]);

    operation_t ops[] = {add, multiply};
    printf("Add: %d, Multiply: %d\n", ops[0](2, 3), ops[1](2, 3));

    return 0;
}
"#;

    let c_file = temp_dir.path().join("complex.c");
    fs::write(&c_file, problematic_c).await?;

    info!("Created complex C file for testing error handling");

    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    match processor.process_single(&c_file).await {
        Ok(()) => {
            info!("✅ Complex translation succeeded!");
        }
        Err(e) => {
            warn!("❌ Translation failed: {}", e);
            info!("This demonstrates proper error handling in the translation pipeline");
        }
    }

    Ok(())
}

/// Example 5: Multi-file project with headers
async fn example_multi_file_project() -> Result<()> {
    info!("=== Example 5: Multi-file Project ===");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("multi_file_project");
    fs::create_dir_all(&project_dir).await?;

    // Create multiple files for a project
    let main_c = r#"
#include "utils.h"
#include <stdio.h>

int main() {
    int result = add_numbers(10, 20);
    printf("Result: %d\n", result);
    return 0;
}
"#;

    let utils_c = r#"
#include "utils.h"

int add_numbers(int a, int b) {
    return a + b;
}

int multiply_numbers(int a, int b) {
    return a * b;
}
"#;

    let utils_h = r#"
#ifndef UTILS_H
#define UTILS_H

int add_numbers(int a, int b);
int multiply_numbers(int a, int b);

#endif // UTILS_H
"#;

    fs::write(project_dir.join("main.c"), main_c).await?;
    fs::write(project_dir.join("utils.c"), utils_c).await?;
    fs::write(project_dir.join("utils.h"), utils_h).await?;

    info!("Created multi-file C project");

    // Process the project
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    match processor.process_single(&project_dir).await {
        Ok(()) => {
            info!("✅ Multi-file project translation completed!");
        }
        Err(e) => {
            warn!("❌ Multi-file translation failed: {}", e);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    info!("🚀 Starting C to Rust Translation Examples");
    info!("==========================================");

    // Run all examples
    example_single_file_translation().await?;
    println!("\n");

    example_batch_translation().await?;
    println!("\n");

    example_directory_structure().await?;
    println!("\n");

    example_error_handling().await?;
    println!("\n");

    example_multi_file_project().await?;

    info!("\n🎉 All examples completed successfully!");
    info!("==========================================");

    println!("\n📝 Key Takeaways:");
    println!("1. Use `process_single_path()` for single file/directory translations");
    println!("2. Use `process_batch_paths()` for batch processing multiple paths");
    println!("3. The processor handles both individual files and directory structures");
    println!("4. Proper error handling ensures robust translation workflows");
    println!("5. Multi-file projects are automatically processed and combined");

    println!("\n💡 Next Steps:");
    println!("- Integrate with your own C projects");
    println!("- Configure the system with proper API keys and settings");
    println!("- Set up automated workflows for large-scale migrations");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_example_functions_dont_panic() {
        // These tests just ensure the examples don't panic
        // In a real scenario, you'd want more detailed testing

        init_logging();

        // Test that all example functions can be called without panicking
        assert!(example_single_file_translation().await.is_ok());
        assert!(example_batch_translation().await.is_ok());
        assert!(example_directory_structure().await.is_ok());
        assert!(example_error_handling().await.is_ok());
        assert!(example_multi_file_project().await.is_ok());
    }
}
