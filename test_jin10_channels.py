#!/usr/bin/env python3
"""
Test different Jin10 channel IDs to find A-share related channels.
Run: python test_jin10_channels.py
"""

import sys
import os

python_runtime_path = os.path.join(os.path.dirname(__file__), 'src-tauri', 'python-runtime', 'scripts')
sys.path.insert(0, python_runtime_path)

from providers.jinshi import news as fetch_jinshi_news

def test_channel(channel_id, channel_name, count=5):
    """Test a specific channel ID."""
    print(f"\n=== 测试频道: {channel_name} (ID: {channel_id}) ===")
    try:
        items = fetch_jinshi_news("", count)
        if not items:
            print(f"  [WARNING] 频道 {channel_id} 返回 0 条数据")
            return

        print(f"  [SUCCESS] 抓取 {len(items)} 条短讯")
        for i, item in enumerate(items[:3]):  # 只显示前3条
            print(f"  {i+1}. {item.get('title', 'N/A')[:80]}...")
    except Exception as e:
        print(f"  [ERROR] 抓取失败: {e}")

def main():
    print("=== 金十数据频道测试 ===")
    print("测试不同的频道ID，找到A股相关的频道")

    # 测试不同的频道ID
    channels = [
        ("-8200", "当前使用（全部频道？）"),
        ("-8888", "全部频道"),
        ("102", "国内经济"),
        ("285", "外汇"),
        ("54", "贵金属"),
        ("55", "原油"),
        ("56", "加密货币"),
        ("353", "美股"),
    ]

    for channel_id, channel_name in channels:
        test_channel(channel_id, channel_name, 5)

if __name__ == "__main__":
    main()
