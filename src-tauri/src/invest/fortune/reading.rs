//! 每日盈记 AI 解读。复用 committee 的 CliCommitteeExecutor::run_role
//! （其内部信号量与委员会共享）；此处额外加 permits=1 闸做防连点串行化。
use std::sync::Arc;
use tokio::sync::Semaphore;
use crate::invest::fortune::aggregate::compute_analysis;
use crate::storage::invest::fortune::insert_reading;

/// 防连点闸：同一时刻最多 1 个 fortune 解读排队（注：CLI 并发仍受委员会共享闸约束）。
static READING_SEM: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();
fn sem() -> Arc<Semaphore> {
    READING_SEM.get_or_init(|| Arc::new(Semaphore::new(1))).clone()
}

pub async fn generate_reading(date: &str) -> Result<String, String> {
    // 取当天卡的干支 + 分数作 prompt 素材
    let analysis = compute_analysis()?;
    let card = analysis.today.as_ref()
        .filter(|c| c.date == date)
        .or(analysis.tomorrow.as_ref())
        .ok_or_else(|| "无当日数据，无法生成解读".to_string())?;
    let sys = "你是一个轻松的股市『每日盈记』解读助手。基于用户给的当日干支与历史统计，\
        用 2-3 句中文给出偏娱乐的吉凶点评，口吻轻松，结尾提醒仅供参考娱乐。不要免责长篇。";
    let user = format!(
        "日期 {}，干支「{}{}」，综合评分 {:.0}。请给一句话点评。",
        card.date, card.stem, card.branch, card.predict_score);

    let semaphore = sem();
    let _permit = semaphore.acquire().await.map_err(|e| format!("信号量获取失败: {e}"))?;
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or_else(|| "未找到 claude CLI，无法生成解读".to_string())?;
    let content = exec.run_role(sys, &user, 60, None, None).await?;
    insert_reading(date, &content)?;
    Ok(content)
}
