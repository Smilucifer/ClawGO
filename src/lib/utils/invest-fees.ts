import type { FeeProfile, Holding, PriceQuote } from "$lib/types";

/** 今日某 symbol 的成交聚合（股数 + 金额）。金额为成交额（price×shares），含费口径由调用方处理。 */
export interface TodayTraded {
  buyShares: number;
  /** 买入支出 = Σ(price×shares + 佣金)，含佣金。 */
  buyCost: number;
  sellShares: number;
  /** 卖出回款 = Σ(price×shares − 佣金)，扣佣金、不扣印花税。 */
  sellProceeds: number;
}

export const EMPTY_TRADED: TodayTraded = {
  buyShares: 0,
  buyCost: 0,
  sellShares: 0,
  sellProceeds: 0,
};

/** 佣金（含过户费，买卖同口径）= max(amount × commission_rate, min_commission) + amount × transfer_fee_rate。 */
export function computeCommission(amount: number, fee: FeeProfile | null): number {
  if (!fee || amount <= 0) return 0;
  const commission = Math.max(amount * fee.commission_rate, fee.min_commission);
  const transfer = amount * fee.transfer_fee_rate;
  return commission + transfer;
}

/** 印花税（仅卖出）= amount × stamp_duty_rate。 */
export function computeStampDuty(amount: number, fee: FeeProfile | null): number {
  if (!fee || amount <= 0) return 0;
  return amount * fee.stamp_duty_rate;
}

/**
 * 单 symbol 当日盈亏（现金流调整盯市法）。
 *
 * ```
 * shares_open = shares_now − today_buy_shares + today_sell_shares
 * 当日盈亏 = close×shares_now + 卖出回款 − pre_close×shares_open − 买入支出
 * pre_close = close − change
 * ```
 *
 * - 佣金通过 buyCost/sellProceeds 计入（现金视角），印花税不计入。
 * - 天然覆盖纯持有 / 新买 / 加仓 / 减仓 / 清仓，无需特例。
 * - 收益率分母 = pre_close×shares_open（昨收开盘市值）；纯新买（无开盘持仓）时
 *   退化为以买入支出为分母，避免除零并给出合理百分比。
 *
 * 报价缺失（quote 为空）返回 null —— 调用方据此跳过该 symbol。
 */
export function computeDailyPnl(
  h: Pick<Holding, "shares">,
  quote: PriceQuote | undefined | null,
  traded: TodayTraded | undefined,
): { amount: number; pct: number; denom: number } | null {
  if (!quote) return null;
  const t = traded ?? EMPTY_TRADED;
  const sharesNow = h.shares ?? 0;
  const close = quote.close;
  const preClose = close - quote.change;
  const sharesOpen = sharesNow - t.buyShares + t.sellShares;

  const amount =
    close * sharesNow + t.sellProceeds - preClose * sharesOpen - t.buyCost;

  // 收益率分母：优先昨收开盘市值；纯新买（无开盘持仓）时用买入支出。
  // 组合层把各 symbol 的 denom 累加后作为整体分母，保证单股/组合口径一致。
  const prevValue = preClose * sharesOpen;
  const denom = prevValue > 0 ? prevValue : t.buyCost;
  const pct = denom > 0 ? (amount / denom) * 100 : 0;

  return { amount, pct, denom };
}
