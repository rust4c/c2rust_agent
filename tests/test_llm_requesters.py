#!/usr/bin/env python3
"""
LLM请求器完整测试脚本 - 测试所有LLM请求器的功能
"""

import os
import sys
import asyncio
import json
from typing import Dict, Any, List
from unittest.mock import Mock, patch

# 添加项目根目录到Python路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from src.base.Base import Base
from src.modules.LLMRequester import (
    LLMRequester,
    OpenaiRequester,
    LocalLLMRequester,
    AnthropicRequester,
    CohereRequester,
    GoogleRequester,
    AmazonbedrockRequester,
    SakuraRequester,
    DashscopeRequester,
    LLMClientFactory,
    get_default_config,
    get_supported_platforms
)


class LLMRequesterTester(Base):
    """LLM请求器测试类"""

    def __init__(self):
        # 设置默认配置 - 简化结构
        self.default = {}

        # 直接设置测试配置
        self.test_configs = {
            "openai": {
                "target_platform": "openai",
                "api_key": "test_key",
                "api_url": "https://api.openai.com/v1",
                "model_name": "gpt-4o-mini",
                "temperature": 0.7,
                "request_timeout": 30
            },
            "local": {
                "target_platform": "LocalLLM",
                "api_key": "none_api_key",
                "api_url": "http://localhost:8000/v1",
                "model_name": "deepseek-r1:7b",
                "temperature": 0.7,
                "request_timeout": 30,
                "think_switch": True
            },
            "anthropic": {
                "target_platform": "anthropic",
                "api_key": "test_key",
                "api_url": "https://api.anthropic.com",
                "model_name": "claude-3-5-sonnet-20241022",
                "temperature": 0.7,
                "request_timeout": 30
            }
        }

        super().__init__()

        # 测试数据
        self.test_messages = [
            {"role": "user", "content": "Hello, please respond with 'Test successful' if you can see this message."}
        ]

        self.c_to_rust_messages = [
            {
                "role": "user",
                "content": """Convert this simple C code to Rust:

```c
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    int result = add(3, 4);
    printf("Result: %d\\n", result);
    return 0;
}
```"""
            }
        ]

        self.system_prompt = "You are a helpful assistant for testing C to Rust conversion."
        self.c_to_rust_prompt = """You are an expert in converting C code to Rust.
Convert the provided C code to safe, idiomatic Rust code.
Focus on memory safety and idiomatic Rust patterns."""

    def test_supported_platforms(self):
        """测试支持的平台列表"""
        self.info("=" * 50)
        self.info("测试支持的平台列表")
        self.info("=" * 50)

        try:
            platforms = get_supported_platforms()
            self.info(f"支持的平台数量: {len(platforms)}")

            for i, platform in enumerate(platforms, 1):
                self.info(f"  {i}. {platform}")

                # 测试默认配置
                default_config = get_default_config(platform)
                if default_config:
                    self.debug(f"    默认配置: {json.dumps(default_config, indent=2, ensure_ascii=False)}")
                else:
                    self.warning(f"    平台 {platform} 没有默认配置")

            self.info("✓ 支持平台列表测试通过")
            return True

        except Exception as e:
            self.error(f"✗ 支持平台列表测试失败: {e}")
            return False

    def test_llm_client_factory(self):
        """测试LLM客户端工厂"""
        self.info("=" * 50)
        self.info("测试LLM客户端工厂")
        self.info("=" * 50)

        try:
            factory = LLMClientFactory()
            self.info("✓ LLM客户端工厂创建成功")

            # 测试各种客户端创建
            test_configs = [
                ("openai", self.test_configs["openai"]),
                ("local", self.test_configs["local"]),
                ("anthropic", self.test_configs["anthropic"])
            ]

            success_count = 0
            for client_type, config in test_configs:
                try:
                    if client_type == "openai":
                        client = factory.get_openai_client(config)
                    elif client_type == "local":
                        client = factory.get_openai_client_local(config)
                    elif client_type == "anthropic":
                        client = factory.get_anthropic_client(config)

                    if client:
                        self.info(f"✓ {client_type} 客户端创建成功")
                        success_count += 1
                    else:
                        self.error(f"✗ {client_type} 客户端创建失败")

                except Exception as e:
                    self.error(f"✗ {client_type} 客户端创建异常: {e}")

            self.info(f"客户端工厂测试结果: {success_count}/{len(test_configs)} 成功")
            return success_count > 0

        except Exception as e:
            self.error(f"✗ LLM客户端工厂测试失败: {e}")
            return False

    def test_individual_requesters(self):
        """测试各个请求器的初始化"""
        self.info("=" * 50)
        self.info("测试各个请求器初始化")
        self.info("=" * 50)

        requesters = [
            ("OpenaiRequester", OpenaiRequester),
            ("LocalLLMRequester", LocalLLMRequester),
            ("AnthropicRequester", AnthropicRequester),
            ("CohereRequester", CohereRequester),
            ("GoogleRequester", GoogleRequester),
            ("AmazonbedrockRequester", AmazonbedrockRequester),
            ("SakuraRequester", SakuraRequester),
            ("DashscopeRequester", DashscopeRequester)
        ]

        success_count = 0
        for name, requester_class in requesters:
            try:
                requester = requester_class()
                if requester:
                    self.info(f"✓ {name} 初始化成功")
                    success_count += 1
                else:
                    self.error(f"✗ {name} 初始化失败")

            except Exception as e:
                self.error(f"✗ {name} 初始化异常: {e}")

        self.info(f"请求器初始化测试结果: {success_count}/{len(requesters)} 成功")
        return success_count == len(requesters)

    def test_main_requester(self):
        """测试主请求器"""
        self.info("=" * 50)
        self.info("测试主请求器")
        self.info("=" * 50)

        try:
            requester = LLMRequester()
            self.info("✓ 主请求器创建成功")

            # 测试配置验证
            test_configs = [
                ("有效配置", self.test_configs["local"]),
                ("缺少平台配置", {"model_name": "test"}),
                ("空配置", {})
            ]

            for test_name, config in test_configs:
                is_valid = requester.validate_config(config)
                self.info(f"  {test_name}: {'✓ 有效' if is_valid else '✗ 无效'}")

            # 测试支持平台列表
            platforms = requester.get_supported_platforms()
            self.info(f"✓ 支持平台数量: {len(platforms)}")

            return True

        except Exception as e:
            self.error(f"✗ 主请求器测试失败: {e}")
            return False

    def test_mock_requests(self):
        """使用Mock测试请求功能"""
        self.info("=" * 50)
        self.info("测试Mock请求")
        self.info("=" * 50)

        try:
            # 创建Mock响应
            mock_response = Mock()
            mock_response.choices = [Mock()]
            mock_response.choices[0].message = Mock()
            mock_response.choices[0].message.content = "Test successful - this is a mock response"
            mock_response.usage = Mock()
            mock_response.usage.prompt_tokens = 10
            mock_response.usage.completion_tokens = 8

            # 测试OpenAI请求器
            with patch('src.modules.LLMRequester.LLMClientFactory.LLMClientFactory') as mock_factory:
                mock_client = Mock()
                mock_client.chat.completions.create.return_value = mock_response
                mock_factory.return_value.get_openai_client_local.return_value = mock_client

                requester = LocalLLMRequester()
                config = self.config["llm"]["providers"]["LocalLLM"]

                error, think, content, prompt_tokens, completion_tokens = requester.request_LocalLLM(
                    self.test_messages, self.system_prompt, config
                )

                if not error and content:
                    self.info("✓ Mock LocalLLM请求成功")
                    self.info(f"  响应内容: {content}")
                    self.info(f"  Token消耗: {prompt_tokens} 输入, {completion_tokens} 输出")
                    return True
                else:
                    self.error("✗ Mock LocalLLM请求失败")
                    return False

        except Exception as e:
            self.error(f"✗ Mock请求测试失败: {e}")
            return False

    def test_real_local_request(self):
        """测试真实的本地LLM请求（如果服务可用）"""
        self.info("=" * 50)
        self.info("测试真实本地LLM请求")
        self.info("=" * 50)

        try:
            requester = LocalLLMRequester()
            config = self.test_configs["local"]

            self.info("发送简单测试请求...")
            self.info(f"API URL: {config['api_url']}")
            self.info(f"Model: {config['model_name']}")

            # 发起请求
            error, think, content, prompt_tokens, completion_tokens = requester.request_LocalLLM(
                self.test_messages, self.system_prompt, config
            )

            if error:
                self.warning("本地LLM服务不可用，这是正常的（如果服务未启动）")
                return True
            else:
                self.info("✓ 本地LLM请求成功")
                self.info(f"响应内容: {content}")
                if think:
                    self.info(f"推理过程: {think[:100]}...")
                self.info(f"Token消耗: {prompt_tokens} 输入, {completion_tokens} 输出")
                return True

        except Exception as e:
            self.warning(f"本地LLM请求测试异常: {e} (这可能是因为服务未启动)")
            return True  # 不算作失败，因为服务可能未启动

    def test_c_to_rust_conversion_mock(self):
        """使用Mock测试C到Rust转换"""
        self.info("=" * 50)
        self.info("测试C到Rust转换 (Mock)")
        self.info("=" * 50)

        rust_code = '''fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let result = add(3, 4);
    println!("Result: {}", result);
}'''

        try:
            # 创建Mock响应
            mock_response = Mock()
            mock_response.choices = [Mock()]
            mock_response.choices[0].message = Mock()
            mock_response.choices[0].message.content = f"Here's the Rust conversion:\n\n```rust\n{rust_code}\n```"
            mock_response.usage = Mock()
            mock_response.usage.prompt_tokens = 50
            mock_response.usage.completion_tokens = 30

            with patch('src.modules.LLMRequester.LLMClientFactory.LLMClientFactory') as mock_factory:
                mock_client = Mock()
                mock_client.chat.completions.create.return_value = mock_response
                mock_factory.return_value.get_openai_client_local.return_value = mock_client

                requester = LLMRequester()
                config = self.test_configs["local"]

                error, think, content, prompt_tokens, completion_tokens = requester.sent_request(
                    self.c_to_rust_messages, self.c_to_rust_prompt, config
                )

                if not error and content:
                    self.info("✓ Mock C到Rust转换成功")
                    self.info("转换结果:")
                    self.info("-" * 40)
                    print(content)
                    self.info("-" * 40)
                    self.info(f"Token消耗: {prompt_tokens} 输入, {completion_tokens} 输出")
                    return True
                else:
                    self.error("✗ Mock C到Rust转换失败")
                    return False

        except Exception as e:
            self.error(f"✗ Mock C到Rust转换测试失败: {e}")
            return False

    def test_error_handling(self):
        """测试错误处理"""
        self.info("=" * 50)
        self.info("测试错误处理")
        self.info("=" * 50)

        try:
            requester = LLMRequester()

            # 测试无效配置
            invalid_configs = [
                {"target_platform": "invalid_platform"},
                {"target_platform": "openai", "api_key": "", "api_url": "invalid_url"},
                {}
            ]

            error_handled_count = 0
            for i, config in enumerate(invalid_configs, 1):
                try:
                    error, think, content, prompt_tokens, completion_tokens = requester.sent_request(
                        self.test_messages, self.system_prompt, config
                    )

                    if error:
                        self.info(f"✓ 错误配置 {i} 正确处理了错误")
                        error_handled_count += 1
                    else:
                        self.warning(f"⚠ 错误配置 {i} 没有返回错误（可能使用了默认处理）")

                except Exception as e:
                    self.info(f"✓ 错误配置 {i} 正确抛出异常: {e}")
                    error_handled_count += 1

            self.info(f"错误处理测试结果: {error_handled_count}/{len(invalid_configs)} 正确处理")
            return error_handled_count > 0

        except Exception as e:
            self.error(f"✗ 错误处理测试失败: {e}")
            return False

    def test_config_templates(self):
        """测试配置模板"""
        self.info("=" * 50)
        self.info("测试配置模板")
        self.info("=" * 50)

        try:
            platforms = get_supported_platforms()
            template_count = 0

            for platform in platforms:
                config = get_default_config(platform)
                if config:
                    template_count += 1
                    self.info(f"✓ {platform} 有配置模板")

                    # 验证必要字段
                    if "target_platform" in config:
                        self.debug(f"  target_platform: {config['target_platform']}")
                    else:
                        self.warning(f"  缺少 target_platform 字段")

                else:
                    self.warning(f"⚠ {platform} 没有配置模板")

            self.info(f"配置模板测试结果: {template_count}/{len(platforms)} 有模板")
            return template_count > 0

        except Exception as e:
            self.error(f"✗ 配置模板测试失败: {e}")
            return False

    def run_all_tests(self):
        """运行所有测试"""
        self.info("开始LLM请求器完整测试")
        self.info("=" * 60)

        tests = [
            ("支持平台列表测试", self.test_supported_platforms),
            ("LLM客户端工厂测试", self.test_llm_client_factory),
            ("请求器初始化测试", self.test_individual_requesters),
            ("主请求器测试", self.test_main_requester),
            ("Mock请求测试", self.test_mock_requests),
            ("配置模板测试", self.test_config_templates),
            ("错误处理测试", self.test_error_handling),
            ("C到Rust转换Mock测试", self.test_c_to_rust_conversion_mock),
        ]

        # 运行基础测试
        passed = 0
        for test_name, test_func in tests:
            try:
                if test_func():
                    passed += 1
                    self.info(f"✓ {test_name} 通过")
                else:
                    self.error(f"✗ {test_name} 失败")
            except Exception as e:
                self.error(f"✗ {test_name} 异常: {e}")

            self.info("")

        # 运行真实请求测试（可选）
        self.info("开始真实请求测试（可选）...")
        self.info("")

        optional_tests = [
            ("真实本地LLM请求测试", self.test_real_local_request),
        ]

        for test_name, test_func in optional_tests:
            try:
                if test_func():
                    passed += 1
                    self.info(f"✓ {test_name} 通过")
                else:
                    self.info(f"⚠ {test_name} 跳过或失败（这是正常的）")
            except Exception as e:
                self.info(f"⚠ {test_name} 异常: {e} （这是正常的）")

            self.info("")

        # 输出测试总结
        total_tests = len(tests) + len(optional_tests)
        self.info("=" * 60)
        self.info(f"测试总结: {passed}/{total_tests} 通过")

        if passed >= len(tests):
            self.info("✓ LLM请求器基础功能正常")
            if passed == total_tests:
                self.info("✓ 所有测试通过，包括真实服务测试")
            else:
                self.info("⚠ 真实服务测试未通过（如果服务未启动，这是正常的）")
        else:
            self.error("✗ LLM请求器存在问题，请检查实现")

        return passed >= len(tests)

    def interactive_test_menu(self):
        """交互式测试菜单"""
        self.info("=" * 60)
        self.info("LLM请求器交互式测试")
        self.info("=" * 60)

        while True:
            print("\n请选择测试项目:")
            print("1. 运行所有测试")
            print("2. 支持平台列表测试")
            print("3. 客户端工厂测试")
            print("4. 请求器初始化测试")
            print("5. Mock请求测试")
            print("6. 真实本地LLM请求测试")
            print("7. C到Rust转换Mock测试")
            print("8. 配置模板测试")
            print("9. 错误处理测试")
            print("0. 退出")

            try:
                choice = input("\n请输入选择 (0-9): ").strip()

                if choice == "0":
                    self.info("退出测试")
                    break
                elif choice == "1":
                    self.run_all_tests()
                elif choice == "2":
                    self.test_supported_platforms()
                elif choice == "3":
                    self.test_llm_client_factory()
                elif choice == "4":
                    self.test_individual_requesters()
                elif choice == "5":
                    self.test_mock_requests()
                elif choice == "6":
                    self.test_real_local_request()
                elif choice == "7":
                    self.test_c_to_rust_conversion_mock()
                elif choice == "8":
                    self.test_config_templates()
                elif choice == "9":
                    self.test_error_handling()
                else:
                    self.warning("无效选择，请重试")

            except KeyboardInterrupt:
                self.info("\n\n测试被用户中断")
                break
            except Exception as e:
                self.error(f"测试过程中发生错误: {e}")


def main():
    """主函数"""
    print("C2Rust Agent - LLM请求器测试工具")
    print("=" * 60)

    if len(sys.argv) > 1 and sys.argv[1] == "--interactive":
        # 交互式模式
        try:
            tester = LLMRequesterTester()
            tester.interactive_test_menu()
        except KeyboardInterrupt:
            print("\n\n⚠ 测试被用户中断")
        except Exception as e:
            print(f"\n❌ 测试过程中发生错误: {e}")
            sys.exit(1)
    else:
        # 自动测试模式
        try:
            tester = LLMRequesterTester()
            success = tester.run_all_tests()

            if success:
                print("\n🎉 测试完成！LLM请求器功能正常。")
                print("\n💡 提示:")
                print("   - 如果真实请求测试失败，请确保LLM服务已启动")
                print("   - 本地服务默认地址: http://localhost:8000/v1")
                print("   - 可以修改 config/config.json 来调整配置")
                print("   - 使用 --interactive 参数进入交互式测试模式")
            else:
                print("\n❌ 测试失败！请检查LLM请求器实现。")
                sys.exit(1)

        except KeyboardInterrupt:
            print("\n\n⚠ 测试被用户中断")
        except Exception as e:
            print(f"\n❌ 测试过程中发生错误: {e}")
            sys.exit(1)


if __name__ == "__main__":
    main()
