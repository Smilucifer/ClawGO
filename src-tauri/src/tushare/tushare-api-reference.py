# Tushare Pro API Reference
# 这是 Python 调用示例，Rust 实现对应此模式

import os
import tushare as ts

pro = ts.pro_api('885e1148928c6b99a505cf1438bca2a01ce5d7c048545f8fa4d488bb')

# ⭐️ 必须设置自定义代理 URL
# 如果显示 Token 不对，请检查代码是不是少了这行
pro._DataApi__http_url = "http://101.35.233.113:8020/"

# 示例: 获取指数基本信息
df = pro.index_basic(limit=5)
print(df)

# 示例: 获取日线行情 (前复权)
df = ts.pro_bar(api=pro, ts_code="000001.SZ", limit=3)
print(df)
