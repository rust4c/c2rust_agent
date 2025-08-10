import os
import re

# 查找所有 C/Cpp 文件
def find_file_with_os_walk(directory, filename, use_regex=False):
    """
    查找文件
    Args:
        directory: 搜索目录
        filename: 文件名或正则表达式模式
        use_regex: 是否使用正则表达式匹配
    Returns:
        str: 找到的第一个匹配文件的完整路径，未找到返回None
    """
    if use_regex:
        pattern = re.compile(filename)
        for root, dirs, files in os.walk(directory):
            for file in files:
                if pattern.search(file):
                    return os.path.join(root, file)
    else:
        for root, dirs, files in os.walk(directory):
            if filename in files:
                return os.path.join(root, filename)
    return None

def find_files_with_regex(directory, pattern):
    """
    使用正则表达式查找所有匹配的文件
    Args:
        directory: 搜索目录
        pattern: 正则表达式模式
    Returns:
        list: 所有匹配文件的完整路径列表
    """
    regex = re.compile(pattern)
    matched_files = []

    for root, dirs, files in os.walk(directory):
        for file in files:
            if regex.search(file):
                matched_files.append(os.path.join(root, file))

    return matched_files

if __name__ == "__main__":
    # 测试原有功能
    print("原有功能测试:")
    print(find_file_with_os_walk("/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc", ".c"))

    # 测试正则表达式匹配
    print("\n正则表达式匹配测试:")
    # 查找所有.c文件
    c_files = find_files_with_regex("/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc", r"\.c$")
    print(f"找到 {len(c_files)} 个.c文件:")
    for file in c_files[:5]:  # 只显示前5个
        print(f"  {file}")

    # 使用增强的函数进行正则匹配
    print("\n使用增强函数的正则匹配:")
    first_c_file = find_file_with_os_walk("/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc", r"\.c$", use_regex=True)
    print(f"第一个.c文件: {first_c_file}")

    # 查找特定模式的文件，如main开头的文件
    main_files = find_files_with_regex("/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc", r"^main.*")
    print(f"\nmain开头的文件 ({len(main_files)}个):")
    for file in main_files:
        print(f"  {file}")
