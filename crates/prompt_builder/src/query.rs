//! Database query functions
//! 
//! All the get_* database query methods extracted from PromptBuilder.
//! Pure data access layer - no formatting, no business logic.

use crate::types::{CallRelationship, FileDependency, FunctionInfo, InterfaceContext};
use anyhow::Result;
use db_services::DatabaseManager;
use log::{debug, warn};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Get file basic information from database
pub async fn get_file_basic_info(
    db_manager: &DatabaseManager,
    file_path: &Path,
    project_name: &str,
) -> Result<serde_json::Value> {
    debug!("Getting file basic info for: {}", file_path.display());

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let query = r#"
        SELECT file_path, language, project, COUNT(*) AS entry_count
        FROM code_entries
        WHERE (file_path = ? OR file_path LIKE ?)
        GROUP BY file_path, language, project
        ORDER BY entry_count DESC
        LIMIT 1
    "#;

    let params = vec![
        json!(file_path.to_string_lossy().to_string()),
        json!(format!("%{}", file_name)),
    ];

    match db_manager.execute_raw_query(query, params).await {
        Ok(results) => {
            if let Some(row) = results.first() {
                Ok(json!({
                    "file_path": row.get("file_path").unwrap_or(&json!("unknown")),
                    "language": row.get("language").unwrap_or(&json!("c")),
                    "project_name": row.get("project").unwrap_or(&json!("unknown")),
                    "interface_count": row.get("entry_count").unwrap_or(&json!(0))
                }))
            } else {
                Ok(json!({
                    "file_path": file_path.to_string_lossy().to_string(),
                    "language": "c",
                    "project_name": project_name,
                    "interface_count": 0
                }))
            }
        }
        Err(e) => {
            warn!("Failed to get file basic info: {}", e);
            Ok(json!({
                "file_path": file_path.to_string_lossy().to_string(),
                "language": "c",
                "project_name": project_name,
                "interface_count": 0
            }))
        }
    }
}

/// Get functions defined in the file
pub async fn get_defined_functions(
    db_manager: &DatabaseManager,
    file_path: &Path,
) -> Result<Vec<FunctionInfo>> {
    debug!("Getting defined functions for: {}", file_path.display());

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let query = r#"
        SELECT ce.file_path AS file_path, ce.function_name AS function_name, ar.result AS result_json
        FROM analysis_results ar
        JOIN code_entries ce ON ce.id = ar.code_id
        WHERE ar.analysis_type = 'function_definition'
          AND (ce.file_path = ? OR ce.file_path LIKE ?)
        ORDER BY ce.updated_at DESC
    "#;

    let params = vec![
        json!(file_path.to_string_lossy().to_string()),
        json!(format!("%{}", file_name)),
    ];

    match db_manager.execute_raw_query(query, params).await {
        Ok(results) => {
            let mut functions: Vec<FunctionInfo> = Vec::new();
            for row in results {
                let file_path_val = row
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let result_json = row
                    .get("result_json")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");

                let parsed: serde_json::Value =
                    serde_json::from_str(result_json).unwrap_or(json!({}));
                let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or(
                    row.get("function_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                );
                let line = parsed
                    .get("line")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32);
                let return_type = parsed
                    .get("return_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let parameters_str = parsed
                    .get("parameters")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let parts: Vec<String> = arr
                            .iter()
                            .map(|p| {
                                let t = p
                                    .get("type")
                                    .or_else(|| p.get("r#type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                let n = p
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("param");
                                format!("{} {}", t, n)
                            })
                            .collect();
                        parts.join(", ")
                    });

                let signature = Some(format!(
                    "{} {}({})",
                    return_type.as_deref().unwrap_or("void"),
                    name,
                    parameters_str.as_deref().unwrap_or("")
                ));

                functions.push(FunctionInfo {
                    name: name.to_string(),
                    file_path: PathBuf::from(file_path_val),
                    line_number: line,
                    return_type,
                    parameters: parameters_str,
                    signature,
                });
            }

            debug!("Found {} defined functions", functions.len());
            Ok(functions)
        }
        Err(e) => {
            warn!("Failed to get defined functions: {}", e);
            Ok(Vec::new())
        }
    }
}

/// Get call relationships for the file
pub async fn get_call_relationships(
    _db_manager: &DatabaseManager,
    file_path: &Path,
    _target_functions: Option<&Vec<String>>,
) -> Result<HashMap<String, Vec<CallRelationship>>> {
    debug!("Getting call relationships for: {}", file_path.display());
    // Not available in current DB schema
    debug!("Call relationships not available in current DB schema");
    Ok(HashMap::new())
}

/// Get file dependencies
pub async fn get_file_dependencies(
    _db_manager: &DatabaseManager,
    file_path: &Path,
) -> Result<Vec<FileDependency>> {
    debug!("Getting file dependencies for: {}", file_path.display());
    // Not available in current DB schema
    debug!("File dependencies not available in current DB schema");
    Ok(Vec::new())
}

/// Get function definition
pub async fn get_function_definition(
    db_manager: &DatabaseManager,
    function_name: &str,
) -> Result<Option<FunctionInfo>> {
    debug!("Getting function definition for: {}", function_name);

    let query = r#"
        SELECT ce.file_path AS file_path, ce.function_name AS function_name, ar.result AS result_json
        FROM code_entries ce
        LEFT JOIN analysis_results ar ON ar.code_id = ce.id AND ar.analysis_type = 'function_definition'
        WHERE ce.function_name = ?
        ORDER BY ce.updated_at DESC
        LIMIT 1
    "#;

    let params = vec![json!(function_name)];

    match db_manager.execute_raw_query(query, params).await {
        Ok(results) => {
            if let Some(row) = results.first() {
                let file_path_val = row
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let result_json = row
                    .get("result_json")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                let parsed: serde_json::Value =
                    serde_json::from_str(result_json).unwrap_or(json!({}));
                let name = parsed
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(function_name);
                let line = parsed
                    .get("line")
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32);
                let return_type = parsed
                    .get("return_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let parameters_str = parsed
                    .get("parameters")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        let parts: Vec<String> = arr
                            .iter()
                            .map(|p| {
                                let t = p
                                    .get("type")
                                    .or_else(|| p.get("r#type"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                let n = p
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("param");
                                format!("{} {}", t, n)
                            })
                            .collect();
                        parts.join(", ")
                    });

                let signature = Some(format!(
                    "{} {}({})",
                    return_type.as_deref().unwrap_or("void"),
                    name,
                    parameters_str.as_deref().unwrap_or("")
                ));

                Ok(Some(FunctionInfo {
                    name: name.to_string(),
                    file_path: PathBuf::from(file_path_val),
                    line_number: line,
                    return_type,
                    parameters: parameters_str,
                    signature,
                }))
            } else {
                Ok(None)
            }
        }
        Err(e) => {
            warn!("Failed to get function definition: {}", e);
            Ok(None)
        }
    }
}

/// Get function callers
pub async fn get_function_callers(
    _db_manager: &DatabaseManager,
    function_name: &str,
) -> Result<Vec<CallRelationship>> {
    debug!("Getting function callers for: {}", function_name);
    // Not available in current DB schema
    debug!("Function callers not available in current DB schema");
    Ok(Vec::new())
}

/// Get function callees
pub async fn get_function_callees(
    _db_manager: &DatabaseManager,
    function_name: &str,
) -> Result<Vec<CallRelationship>> {
    debug!("Getting function callees for: {}", function_name);
    // Not available in current DB schema
    debug!("Function callees not available in current DB schema");
    Ok(Vec::new())
}

/// Get interface context from vector database
pub async fn get_interface_context(
    db_manager: &DatabaseManager,
    file_path: &Path,
    project_name: &str,
) -> Result<Vec<InterfaceContext>> {
    debug!("Getting interface context for: {}", file_path.display());

    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let interfaces = db_manager
        .search_interfaces_by_name("", Some(project_name))
        .await?;

    let mut relevant_interfaces = Vec::new();
    for interface in interfaces {
        let interface_file = Path::new(&interface.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if file_path.to_string_lossy() == interface.file_path || file_name == interface_file {
            relevant_interfaces.push(InterfaceContext {
                name: interface.name,
                file_path: PathBuf::from(interface.file_path),
                language: interface.language,
                inputs: interface
                    .inputs
                    .into_iter()
                    .map(|input| format!("{:?}", input))
                    .collect(),
                outputs: interface
                    .outputs
                    .into_iter()
                    .map(|output| format!("{:?}", output))
                    .collect(),
            });
        }
    }

    // Limit to avoid overwhelming prompts
    relevant_interfaces.truncate(10);

    debug!("Found {} relevant interfaces", relevant_interfaces.len());
    Ok(relevant_interfaces)
}
