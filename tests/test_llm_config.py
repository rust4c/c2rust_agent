#!/usr/bin/env python3
"""
LLM配置测试脚本 - 用于验证和测试LLM相关配置
"""

import os
import sys
import asyncio
from typing import Dict, Any

# 添加项目根目录到Python路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'src'))

from src.base.Base import Base
from src.modules.LLMRequester.LocalLLMRequester import LocalLLMRequester
from src.modules.LLMRequester.LLMClientFactory import LLMClientFactory


class LLMConfigTester(Base):
    """LLM配置测试器"""

    def __init__(self):
        # 设置默认配置
        self.default = {
            "llm": {
                "default_provider": "openai_local",
                "providers": {
                    "openai_local": {
                        "api_key": "none_api_key",
                        "api_url": "http://localhost:8000/v1",
                        "model_name": "deepseek-r1:7b",
                        "temperature": 0.7,
                        "top_p": 1.0,
                        "frequency_penalty": 0,
                        "request_timeout": 30,
                        "think_switch": True
                    }
                }
            }
        }

        super().__init__()
        self.config = self.load_config_from_default()
        self.save_config(self.config)

    def test_config_loading(self):
        """测试配置文件加载"""
        self.info("=" * 50)
        self.info("测试配置文件加载")
        self.info("=" * 50)

        try:
            config = self.load_config()
            self.info(f"配置文件路径: {self.CONFIG_PATH}")
            self.info(f"配置文件存在: {os.path.exists(self.CONFIG_PATH)}")

            if "llm" in config:
                self.info("✓ LLM配置加载成功")
                default_provider = config["llm"].get("default_provider", "未设置")
                self.info(f"默认LLM提供商: {default_provider}")

                providers = config["llm"].get("providers", {})
                self.info(f"配置的提供商数量: {len(providers)}")

                for provider_name in providers:
                    provider_config = providers[provider_name]
                    model = provider_config.get("model_name", "未知")
                    api_url = provider_config.get("api_url", "未设置")
                    self.info(f"  - {provider_name}: {model} ({api_url})")
            else:
                self.error("✗ LLM配置不存在")
                return False

        except Exception as e:
            self.error(f"✗ 配置加载失败: {e}")
            return False

        return True

    def test_client_factory(self):
        """测试客户端工厂"""
        self.info("=" * 50)
        self.info("测试LLM客户端工厂")
        self.info("=" * 50)

        try:
            factory = LLMClientFactory()
            self.info("✓ LLM客户端工厂创建成功")

            # 测试本地OpenAI客户端创建
            config = self.config["llm"]["providers"]["openai_local"]
            client = factory.get_openai_client_local(config)

            if client:
                self.info("✓ 本地OpenAI客户端创建成功")
                self.info(f"API URL: {client.base_url}")
                return True
            else:
                self.error("✗ 本地OpenAI客户端创建失败")
                return False

        except Exception as e:
            self.error(f"✗ 客户端工厂测试失败: {e}")
            return False

    def test_llm_requester(self):
        """测试LLM请求器"""
        self.info("=" * 50)
        self.info("测试LLM请求器")
        self.info("=" * 50)

        try:
            requester = LocalLLMRequester()
            self.info("✓ LLM请求器创建成功")
            return True

        except Exception as e:
            self.error(f"✗ LLM请求器测试失败: {e}")
            return False

    def test_simple_request(self):
        """测试简单的LLM请求"""
        self.info("=" * 50)
        self.info("测试简单的LLM请求")
        self.info("=" * 50)

        try:
            requester = LocalLLMRequester()
            config = self.config["llm"]["providers"]["openai_local"]

            messages = [
                {"role": "user", "content": "Hello, please respond with 'Test successful' if you can see this message."}
            ]

            system_prompt = "You are a helpful assistant for testing C to Rust conversion."

            self.info("发送测试请求...")
            self.info(f"API URL: {config['api_url']}")
            self.info(f"Model: {config['model_name']}")

            # 发起请求
            error, think, content, prompt_tokens, completion_tokens = requester.request_LocalLLM(
                messages=messages,
                system_prompt=system_prompt,
                platform_config=config
            )

            if error:
                self.error("✗ LLM请求失败")
                return False
            else:
                self.info("✓ LLM请求成功")
                if think:
                    self.info(f"推理过程: {think[:100]}...")
                self.info(f"回复内容: {content}")
                self.info(f"输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")
                return True

        except Exception as e:
            self.error(f"✗ LLM请求测试失败: {e}")
            return False

    def test_c_to_rust_prompt(self):
        """测试C到Rust转换提示"""
        self.info("=" * 50)
        self.info("测试C到Rust转换提示")
        self.info("=" * 50)

        try:
            requester = LocalLLMRequester()
            config = self.config["llm"]["providers"]["openai_local"]

            c_code = """
#include <stdio.h>
#include <stdlib.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    int result = add(3, 4);
    printf("Result: %d\\n", result);
    return 0;
}
"""

            messages = [
                {
                    "role": "user",
                    "content": f"Please convert this C code to Rust:\n\n```c\n{c_code}\n```"
                }
            ]

            system_prompt = """You are an expert in converting C code to Rust.
Convert the provided C code to safe, idiomatic Rust code.
Provide only the Rust code without explanations."""

            self.info("发送C到Rust转换请求...")

            # 发起请求
            error, think, content, prompt_tokens, completion_tokens = requester.request_LocalLLM(
                messages=messages,
                system_prompt=system_prompt,
                platform_config=config
            )

            if error:
                self.error("✗ C到Rust转换请求失败")
                return False
            else:
                self.info("✓ C到Rust转换请求成功")
                if think:
                    self.info(f"推理过程长度: {len(think)} 字符")
                self.info("转换结果:")
                self.info("-" * 40)
                print(content)
                self.info("-" * 40)
                self.info(f"输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")
                return True

        except Exception as e:
            self.error(f"✗ C到Rust转换测试失败: {e}")
            return False

    def run_all_tests(self):
        """运行所有测试"""
        self.info("开始LLM配置测试")
        self.info("=" * 60)

        tests = [
            ("配置加载测试", self.test_config_loading),
            ("客户端工厂测试", self.test_client_factory),
            ("请求器测试", self.test_llm_requester),
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

        # 如果基础测试都通过，运行实际请求测试
        if passed == len(tests):
            self.info("基础测试全部通过，开始实际请求测试...")
            self.info("")

            request_tests = [
                ("简单请求测试", self.test_simple_request),
                ("C到Rust转换测试", self.test_c_to_rust_prompt),
            ]

            for test_name, test_func in request_tests:
                try:
                    if test_func():
                        passed += 1
                        self.info(f"✓ {test_name} 通过")
                    else:
                        self.error(f"✗ {test_name} 失败 (这可能是因为LLM服务未启动)")
                except Exception as e:
                    self.error(f"✗ {test_name} 异常: {e} (这可能是因为LLM服务未启动)")

                self.info("")

        # 输出测试总结
        total_tests = len(tests) + 2  # 基础测试 + 2个请求测试
        self.info("=" * 60)
        self.info(f"测试总结: {passed}/{total_tests} 通过")

        if passed >= len(tests):
            self.info("✓ LLM配置基础功能正常")
            if passed == total_tests:
                self.info("✓ LLM服务连接正常")
            else:
                self.info("⚠ LLM服务连接失败，请检查服务是否启动")
        else:
            self.error("✗ LLM配置存在问题，请检查配置")

        return passed >= len(tests)


def main():
    """主函数"""
    print("C2Rust Agent - LLM配置测试工具")
    print("=" * 60)

    try:
        tester = LLMConfigTester()
        success = tester.run_all_tests()

        if success:
            print("\n🎉 测试完成！基础配置正常。")
            print("\n💡 提示:")
            print("   - 如果请求测试失败，请确保LLM服务已启动")
            print("   - 本地服务默认地址: http://localhost:8000/v1")
            print("   - 可以修改 config/config.json 来调整配置")
        else:
            print("\n❌ 测试失败！请检查配置和依赖。")
            sys.exit(1)

    except KeyboardInterrupt:
        print("\n\n⚠ 测试被用户中断")
    except Exception as e:
        print(f"\n❌ 测试过程中发生错误: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
