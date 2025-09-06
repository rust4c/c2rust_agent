//! Complete C to Rust translation workflow example
//!
//! This example demonstrates how to use the main_processor crate
//! to translate C code to Rust with proper error handling,
//! progress tracking, and code formatting.

use anyhow::Result;
use log::{info, warn, LevelFilter};
use main_processor::{
    translate_c_file, translate_c_project, ProjectTranslationResult, TranslationAPI,
    TranslationConfig,
};
use std::path::Path;
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
fn init_logging() {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();
}

/// Example 1: Quick single file translation
async fn example_single_file_translation() -> Result<()> {
    info!("=== Example 1: Single File Translation ===");

    // Create a temporary C file
    let temp_dir = TempDir::new()?;
    let c_file_path = temp_dir.path().join("person.c");

    fs::write(&c_file_path, EXAMPLE_C_CODE).await?;
    info!("Created test C file: {}", c_file_path.display());

    // Translate using the convenience function
    match translate_c_file(&c_file_path, Some(temp_dir.path())).await {
        Ok(rust_code) => {
            info!("✅ Translation successful!");
            info!("Generated Rust code preview (first 300 chars):");
            let preview = if rust_code.len() > 300 {
                format!("{}...", &rust_code[..300])
            } else {
                rust_code
            };
            println!("{}", preview);
        }
        Err(e) => {
            warn!("❌ Translation failed: {}", e);
        }
    }

    Ok(())
}

/// Example 2: Advanced configuration with custom settings
async fn example_advanced_translation() -> Result<()> {
    info!("=== Example 2: Advanced Translation Configuration ===");

    let temp_dir = TempDir::new()?;
    let project_dir = temp_dir.path().join("c_project");
    fs::create_dir_all(&project_dir).await?;

    // Create multiple C files for a project
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

    // Configure translation with custom settings
    let config = TranslationConfig {
        test_mode: true, // Use test mode for this example
        max_retries: 5,
        verbose: true,
        cache_dir: Some(temp_dir.path().join("cache").to_string_lossy().to_string()),
        use_database: false, // Disable database for simplicity
    };

    let api = TranslationAPI::new(config);

    // Translate the project
    let result = api
        .translate_project(&project_dir, Some(temp_dir.path()))
        .await?;

    display_translation_result(&result);

    Ok(())
}

/// Example 3: Batch translation of multiple projects
async fn example_batch_translation() -> Result<()> {
    info!("=== Example 3: Batch Translation ===");

    let temp_dir = TempDir::new()?;

    // Create multiple small C projects
    let projects = vec![
        (
            "hello_world",
            "int main() { printf(\"Hello, World!\\n\"); return 0; }",
        ),
        (
            "calculator",
            r#"
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
        fs::write(
            project_path.join("main.c"),
            format!("#include <stdio.h>\n{}", code),
        )
        .await?;
        project_paths.push(project_path);
    }

    info!("Created {} test projects", project_paths.len());

    let config = TranslationConfig {
        test_mode: true,
        ..Default::default()
    };
    let api = TranslationAPI::new(config);

    // Perform batch translation
    let results = api.translate_batch(project_paths).await?;

    // Display summary
    info!("=== Batch Translation Results ===");
    for (i, result) in results.iter().enumerate() {
        println!(
            "Project {}: {} - {}",
            i + 1,
            result.project_name,
            if result.success {
                "✅ Success"
            } else {
                "❌ Failed"
            }
        );

        if !result.success {
            if let Some(error) = &result.error_message {
                println!("  Error: {}", error);
            }
        }
    }

    let stats = TranslationAPI::get_translation_summary(&results);
    println!(
        "\n📊 Summary: {} successful, {} failed",
        stats.successful_translations, stats.failed_translations
    );

    Ok(())
}

/// Example 4: Auto-discovery and translation
async fn example_auto_discovery() -> Result<()> {
    info!("=== Example 4: Auto-Discovery Translation ===");

    let temp_dir = TempDir::new()?;
    let root_dir = temp_dir.path().join("c_projects_root");
    fs::create_dir_all(&root_dir).await?;

    // Create a directory structure with multiple C projects
    let structure = vec![
        (
            "project_a/main.c",
            "#include <stdio.h>\nint main() { printf(\"Project A\\n\"); return 0; }",
        ),
        (
            "project_b/calculator.c",
            "int add(int a, int b) { return a + b; }\nint main() { return add(1,2); }",
        ),
        (
            "project_c/utils.c",
            "void helper() {}\nint main() { helper(); return 0; }",
        ),
    ];

    for (path, content) in structure {
        let full_path = root_dir.join(path);
        fs::create_dir_all(full_path.parent().unwrap()).await?;
        fs::write(&full_path, content).await?;
    }

    info!("Created directory structure with multiple projects");

    let config = TranslationConfig {
        test_mode: true,
        verbose: true,
        ..Default::default()
    };
    let api = TranslationAPI::new(config);

    // Auto-discover and translate
    let results = api.auto_discover_and_translate(&root_dir).await?;

    info!("Auto-discovered and translated {} projects", results.len());

    for result in &results {
        display_translation_result(result);
    }

    Ok(())
}

/// Helper function to display translation results
fn display_translation_result(result: &ProjectTranslationResult) {
    println!("\n--- Translation Result for '{}' ---", result.project_name);
    println!(
        "Status: {}",
        if result.success {
            "✅ Success"
        } else {
            "❌ Failed"
        }
    );

    if let Some(error) = &result.error_message {
        println!("Error: {}", error);
    }

    if !result.warnings.is_empty() {
        println!("Warnings:");
        for warning in &result.warnings {
            println!("  ⚠️  {}", warning);
        }
    }

    if let Some(rust_code) = &result.rust_code {
        println!("Generated Rust code size: {} bytes", rust_code.len());
        // Show a small preview
        if rust_code.len() > 200 {
            println!("Code preview:\n{}", &rust_code[..200]);
            println!("... (truncated)");
        } else {
            println!("Full code:\n{}", rust_code);
        }
    }
}

/// Example 5: Error handling and retry scenarios
async fn example_error_handling() -> Result<()> {
    info!("=== Example 5: Error Handling and Retry ===");

    let temp_dir = TempDir::new()?;

    // Create a problematic C file (intentionally malformed for testing)
    let problematic_c = r#"
// This C code has issues that might cause translation problems
#include <stdio.h>
#include <nonexistent_header.h>  // This header doesn't exist

// Undefined behavior examples
int* dangerous_function() {
    int local_var = 42;
    return &local_var;  // Returning address of local variable
}

void memory_leak() {
    char* buffer = malloc(1000);
    // Forgot to free buffer
}

int main() {
    int* ptr = dangerous_function();
    printf("Value: %d\n", *ptr);  // Undefined behavior
    memory_leak();
    return 0;
}
"#;

    let c_file = temp_dir.path().join("problematic.c");
    fs::write(&c_file, problematic_c).await?;

    info!("Created problematic C file for testing error handling");

    let config = TranslationConfig {
        test_mode: true, // Even in test mode, we can simulate error handling
        max_retries: 2,
        verbose: true,
        ..Default::default()
    };

    let api = TranslationAPI::new(config);

    match api
        .translate_single_file(&c_file, Some(temp_dir.path()))
        .await
    {
        Ok(result) => {
            display_translation_result(&result);
            if result.success {
                info!("Translation succeeded despite problematic code!");
            }
        }
        Err(e) => {
            warn!("Translation failed as expected: {}", e);
            info!("This demonstrates proper error handling in the translation pipeline");
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

    example_advanced_translation().await?;
    println!("\n");

    example_batch_translation().await?;
    println!("\n");

    example_auto_discovery().await?;
    println!("\n");

    example_error_handling().await?;

    info!("\n🎉 All examples completed successfully!");
    info!("==========================================");

    println!("\n📝 Key Takeaways:");
    println!("1. Use `translate_c_file()` for quick single file translations");
    println!("2. Use `TranslationAPI` with custom config for advanced scenarios");
    println!("3. Batch translation handles multiple projects efficiently");
    println!("4. Auto-discovery can find and translate entire directory trees");
    println!("5. Proper error handling ensures robust translation workflows");
    println!("\n💡 Next Steps:");
    println!("- Integrate with your own C projects");
    println!("- Configure database support for context-aware translations");
    println!("- Set up CI/CD pipelines for automated C-to-Rust migration");

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
        assert!(example_advanced_translation().await.is_ok());
        assert!(example_batch_translation().await.is_ok());
        assert!(example_auto_discovery().await.is_ok());
        assert!(example_error_handling().await.is_ok());
    }
}
