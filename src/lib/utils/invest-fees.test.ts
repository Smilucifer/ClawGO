import { describe, expect, it } from "vitest";
import {
  computeDailyPnl,
  computeCommission,
  computeStampDuty,
  type TodayTraded,
} from "./invest-fees";
import type { FeeProfile, PriceQuote } from "$lib/types";

/** 构造 PriceQuote：close + change（pre_close = close − change）。 */
function quote(close: number, change: number): PriceQuote {
  return { tsCode: "x", name: "x", close, change, pctChg: 0, vol: 0, amount: 0 };
}

function traded(p: Partial<TodayTraded>): TodayTraded {
  return { buyShares: 0, buyCost: 0, sellShares: 0, sellProceeds: 0, ...p };
}

describe("computeDailyPnl — 盯市口径", () => {
  it("纯持有（今日无交易）：(close − preClose) × shares", () => {
    // 昨收 10，现价 10.8，1000 股 → (10.8 − 10) × 1000 = 800
    const r = computeDailyPnl({ shares: 1000 }, quote(10.8, 0.8), undefined);
    expect(r?.amount).toBeCloseTo(800, 6);
    // 分母 = preClose × sharesOpen = 10 × 1000 = 10000 → 8%
    expect(r?.pct).toBeCloseTo(8, 6);
  });

  it("当日新买入：只算买入价到现价的涨幅，不含昨收→买入价那段", () => {
    // 昨收 10，买入价 10.5，现价 10.8，1000 股。真实当日盈亏 = (10.8 − 10.5) × 1000 = 300
    // shares_open = 1000 − 1000 + 0 = 0；买入支出 = 10.5×1000 = 10500
    // pnl = 10.8×1000 + 0 − 10×0 − 10500 = 300
    const r = computeDailyPnl(
      { shares: 1000 },
      quote(10.8, 0.8),
      traded({ buyShares: 1000, buyCost: 10500 }),
    );
    expect(r?.amount).toBeCloseTo(300, 6);
    // 纯新买分母退化为买入支出 10500 → 300/10500 ≈ 2.857%
    expect(r?.pct).toBeCloseTo((300 / 10500) * 100, 6);
  });

  it("当日加仓：昨日持仓按昨收基准 + 今日新仓按买入价基准", () => {
    // 昨日持有 1000 股（昨收 10），今日加仓 500 股 @10.5，现价 10.8，现 shares = 1500
    // shares_open = 1500 − 500 + 0 = 1000；买入支出 = 10.5×500 = 5250
    // pnl = 10.8×1500 + 0 − 10×1000 − 5250 = 16200 − 10000 − 5250 = 950
    // 校验：昨仓 (10.8−10)×1000=800 + 新仓 (10.8−10.5)×500=150 = 950 ✓
    const r = computeDailyPnl(
      { shares: 1500 },
      quote(10.8, 0.8),
      traded({ buyShares: 500, buyCost: 5250 }),
    );
    expect(r?.amount).toBeCloseTo(950, 6);
  });

  it("当日部分减仓：保留已卖出部分的当日已实现盈亏", () => {
    // 昨日持有 1000 股（昨收 10），今日卖出 400 股 @10.6，现价 10.8，现 shares = 600
    // shares_open = 600 − 0 + 400 = 1000；卖出回款 = 10.6×400 = 4240
    // pnl = 10.8×600 + 4240 − 10×1000 − 0 = 6480 + 4240 − 10000 = 720
    // 校验：持有 600×(10.8−10)=480 + 已卖 400×(10.6−10)=240 = 720 ✓
    const r = computeDailyPnl(
      { shares: 600 },
      quote(10.8, 0.8),
      traded({ sellShares: 400, sellProceeds: 4240 }),
    );
    expect(r?.amount).toBeCloseTo(720, 6);
  });

  it("当日清仓：shares_now=0，close 项归零，纯看卖出价 vs 昨收", () => {
    // 昨日持有 1000 股（昨收 10），今日全部卖出 @10.6，现 shares = 0
    // shares_open = 0 − 0 + 1000 = 1000；卖出回款 = 10.6×1000 = 10600
    // pnl = 0 + 10600 − 10×1000 − 0 = 600
    // 校验：1000×(10.6−10) = 600 ✓（与现价无关，旧 bug 已消失）
    const r = computeDailyPnl(
      { shares: 0 },
      quote(10.8, 0.8),
      traded({ sellShares: 1000, sellProceeds: 10600 }),
    );
    expect(r?.amount).toBeCloseTo(600, 6);
  });

  it("佣金计入当日盈亏（现金视角）：买入佣金拉低盈亏", () => {
    // 同新买场景但买入支出含 5 元佣金：buyCost = 10500 + 5 = 10505
    // pnl = 10.8×1000 − 10505 = 295（比无佣金少 5）
    const r = computeDailyPnl(
      { shares: 1000 },
      quote(10.8, 0.8),
      traded({ buyShares: 1000, buyCost: 10505 }),
    );
    expect(r?.amount).toBeCloseTo(295, 6);
  });

  it("报价缺失返回 null", () => {
    expect(computeDailyPnl({ shares: 1000 }, undefined, undefined)).toBeNull();
    expect(computeDailyPnl({ shares: 1000 }, null, undefined)).toBeNull();
  });

  it("零持仓零交易：amount 0, pct 0", () => {
    const r = computeDailyPnl({ shares: 0 }, quote(10, 0), undefined);
    expect(r?.amount).toBeCloseTo(0, 6);
    expect(r?.pct).toBe(0);
  });
});

describe("手续费计算", () => {
  const fee: FeeProfile = {
    id: "f1",
    name: "默认",
    commission_rate: 0.00025, // 万 2.5
    min_commission: 5,
    stamp_duty_rate: 0.0005, // 万 5（卖出）
    transfer_fee_rate: 0.00001, // 十万 1
  };

  it("佣金取 max(费率, 最低佣金)，并加过户费", () => {
    // 小额：amount=1000 → 费率佣金 0.25 < 5，取 5；过户费 1000×0.00001=0.01 → 5.01
    expect(computeCommission(1000, fee)).toBeCloseTo(5.01, 6);
    // 大额：amount=100000 → 费率佣金 25 > 5，取 25；过户费 1 → 26
    expect(computeCommission(100000, fee)).toBeCloseTo(26, 6);
  });

  it("印花税仅按费率，无最低", () => {
    expect(computeStampDuty(100000, fee)).toBeCloseTo(50, 6);
  });

  it("无方案或非正金额返回 0", () => {
    expect(computeCommission(1000, null)).toBe(0);
    expect(computeCommission(0, fee)).toBe(0);
    expect(computeStampDuty(0, fee)).toBe(0);
  });
});
