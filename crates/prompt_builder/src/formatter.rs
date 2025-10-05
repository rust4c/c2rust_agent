//! Formatting utilities for building prompt sections
//!
//! All the format_* functions extracted from the bloated lib.rs.
//! Each function does ONE thing: format data into readable text.

use crate::types::{CallRelationship, FileDependency, FunctionInfo, InterfaceContext};
use std::collections::HashMap;

/// Format file basic information
pub fn format_file_info(file_info: &serde_json::Value) -> String {
    format!(
        "## 文件信息\n- 文件路径: {}\n- 编程语言: {}\n- 项目名称: {}\n- 接口数量: {}\n",
        file_info
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        file_info
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        file_info
            .get("project_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        file_info
            .get("interface_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    )
}

/// Format functions defined in a file
pub fn format_defined_functions(functions: &[FunctionInfo]) -> String {
    if functions.is_empty() {
        return String::new();
    }

    let mut section = "## 文件中定义的函数\n".to_string();
    for func in functions {
        section.push_str(&format!(
            "\n### {} (行 {})\n- 返回类型: {}\n- 函数签名: `{}`\n- 参数: {}\n",
            func.name,
            func.line_number.unwrap_or(0),
            func.return_type.as_deref().unwrap_or("unknown"),
            func.signature.as_deref().unwrap_or(&func.name),
            func.parameters.as_deref().unwrap_or("void")
        ));
    }
    section
}

/// Format function call relationships
pub fn format_call_relationships(relationships: &HashMap<String, Vec<CallRelationship>>) -> String {
    if relationships.is_empty() {
        return String::new();
    }

    let mut section = "## 函数调用关系\n".to_string();

    if let Some(internal_calls) = relationships.get("internal_calls") {
        if !internal_calls.is_empty() {
            section.push_str("### 文件内部调用\n");
            for call in internal_calls {
                section.push_str(&format!(
                    "- `{}` 调用 `{}` (行 {})\n",
                    call.caller,
                    call.called,
                    call.line.unwrap_or(0)
                ));
            }
        }
    }

    if let Some(external_calls) = relationships.get("external_calls") {
        if !external_calls.is_empty() {
            section.push_str("\n### 外部文件调用\n");
            for call in external_calls {
                let caller_file = call
                    .caller_file
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                section.push_str(&format!(
                    "- `{}:{}` 调用 `{}` (行 {})\n",
                    caller_file,
                    call.caller,
                    call.called,
                    call.line.unwrap_or(0)
                ));
            }
        }
    }

    section
}

/// Format file dependencies
pub fn format_file_dependencies(dependencies: &[FileDependency]) -> String {
    if dependencies.is_empty() {
        return String::new();
    }

    let mut section = "## 文件依赖关系\n".to_string();
    for dep in dependencies.iter().take(10) {
        let source_file = dep
            .from
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| dep.from.to_string_lossy().into_owned());
        let target_file = dep
            .to
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| dep.to.to_string_lossy().into_owned());

        section.push_str(&format!(
            "- `{}` → `{}` ({})\n",
            source_file, target_file, dep.dependency_type
        ));
    }
    section
}

/// Format interface context information
pub fn format_interface_context(interfaces: &[InterfaceContext]) -> String {
    if interfaces.is_empty() {
        return String::new();
    }

    let mut section = "## 相关接口信息\n".to_string();
    for interface in interfaces.iter().take(5) {
        let file_name = interface
            .file_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| interface.file_path.to_string_lossy().into_owned());

        section.push_str(&format!(
            "\n### {}\n- 文件: {}\n- 语言: {}\n",
            interface.name, file_name, interface.language
        ));
    }
    section
}

/// Format function definition
pub fn format_function_definition(func_def: &FunctionInfo) -> String {
    let file_name = func_def
        .file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| func_def.file_path.to_string_lossy().into_owned());

    format!(
        "## 函数定义\n- 函数名: {}\n- 文件: {}\n- 行号: {}\n- 返回类型: {}\n- 函数签名: `{}`\n- 参数: {}\n",
        func_def.name,
        file_name,
        func_def.line_number.unwrap_or(0),
        func_def.return_type.as_deref().unwrap_or("unknown"),
        func_def.signature.as_deref().unwrap_or(&func_def.name),
        func_def.parameters.as_deref().unwrap_or("void")
    )
}

/// Format function callers
pub fn format_function_callers(callers: &[CallRelationship]) -> String {
    if callers.is_empty() {
        return String::new();
    }

    let mut section = "## 调用该函数的位置\n".to_string();
    for caller in callers {
        let caller_file = caller
            .caller_file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        section.push_str(&format!(
            "- `{}:{}` (行 {})\n",
            caller_file,
            caller.caller,
            caller.line.unwrap_or(0)
        ));
    }
    section
}

/// Format function callees
pub fn format_function_callees(callees: &[CallRelationship]) -> String {
    if callees.is_empty() {
        return String::new();
    }

    let mut section = "## 该函数调用的其他函数\n".to_string();
    for callee in callees {
        let called_file = callee
            .called_file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        section.push_str(&format!(
            "- `{}` 在 `{}` (行 {})\n",
            callee.called,
            called_file,
            callee.line.unwrap_or(0)
        ));
    }
    section
}

/// Format error message
pub fn format_error_message(error_message: &str) -> String {
    format!(
        "## 错误信息\n在上一次构建中,发生错误信息: {}\n",
        error_message
    )
}

/// Build complete file conversion prompt
pub fn build_file_prompt(file_path: &str, sections: &[String], conversion_guide: &str) -> String {
    let header = format!(
        "# C到Rust转换上下文信息\n\n正在转换文件: **{}**\n\n以下是基于项目调用关系分析得到的上下文信息，请在转换过程中参考这些信息以保持函数调用关系和接口一致性。\n\n",
        file_path
    );

    let content = sections.join("\n");

    format!("{}{}\n\n{}", header, content, conversion_guide)
}

/// Build function conversion prompt
pub fn build_function_prompt(
    function_name: &str,
    sections: &[String],
    conversion_guide: &str,
) -> String {
    let header = format!(
        "# 函数转换上下文信息\n\n正在转换函数: **{}**\n\n以下是该函数的调用关系和上下文信息：\n\n",
        function_name
    );

    let content = sections.join("\n");

    format!("{}{}\n\n{}", header, content, conversion_guide)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_defined_functions() {
        let functions = vec![FunctionInfo {
            name: "test_func".to_string(),
            file_path: PathBuf::from("test.c"),
            line_number: Some(10),
            return_type: Some("int".to_string()),
            parameters: Some("void".to_string()),
            signature: Some("int test_func(void)".to_string()),
        }];

        let result = format_defined_functions(&functions);
        assert!(result.contains("test_func"));
        assert!(result.contains("行 10"));
    }

    #[test]
    fn test_format_empty_functions() {
        let result = format_defined_functions(&[]);
        assert!(result.is_empty());
    }
}
