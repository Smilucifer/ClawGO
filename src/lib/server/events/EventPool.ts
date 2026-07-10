import { EventEmitter } from 'events';

export interface PoolEvent {
  id: string;
  timestamp: Date;
  type: string;
  data: Record<string, unknown>;
}

export interface CacheData {
  events: PoolEvent[];
  lastTimestamp: Date;
}

export class EventPool extends EventEmitter {
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  private all: PoolEvent[] = [];
  private lastTimestamp: Date = new Date(0);
  private subscribers: Set<string> = new Set();
  private isRefreshing = false;
  private isConnected = false;
  private cache: CacheData | null = null;

  /**
   * 获取是否正在轮询
   */
  get isPolling(): boolean {
    return this.isConnected && this.refreshTimer !== null;
  }

  /**
   * 获取是否就绪（已连接且有事件数据）
   */
  get isReady(): boolean {
    return this.isConnected && this.lastTimestamp.getTime() > 0;
  }

  /**
   * 连接到事件源
   */
  connect(): void {
    if (this.isConnected) {
      return;
    }
    this.isConnected = true;
  }

  /**
   * 断开连接
   */
  disconnect(): void {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
    this.isConnected = false;
  }

  /**
   * 订阅事件
   */
  subscribe(subscriberId: string): void {
    this.subscribers.add(subscriberId);
  }

  /**
   * 取消订阅
   */
  unsubscribe(subscriberId: string): void {
    this.subscribers.delete(subscriberId);
  }

  /**
   * 从缓存加载事件
   */
  loadFromCache(cacheData: CacheData): boolean {
    try {
      this.all = cacheData.events.map(event => ({
        ...event,
        timestamp: new Date(event.timestamp)
      }));
      this.lastTimestamp = new Date(cacheData.lastTimestamp);
      this.cache = cacheData;

      return true;
    } catch (error) {
      console.error('Failed to load from cache:', error);
      return false;
    }
  }

  /**
   * 保存事件到缓存
   */
  saveToCache(): CacheData {
    const cacheData: CacheData = {
      events: this.all,
      lastTimestamp: this.lastTimestamp
    };
    this.cache = cacheData;
    return cacheData;
  }

  /**
   * 获取缓存数据
   */
  getCacheData(): CacheData | null {
    return this.cache;
  }

  /**
   * 刷新事件数据
   */
  async refresh(fetchFn?: () => Promise<PoolEvent[]>): Promise<void> {
    if (this.isRefreshing) {
      return;
    }
    this.isRefreshing = true;

    try {
      // 如果提供了 fetch 函数，使用它获取新事件
      if (fetchFn) {
        const newEvents = await fetchFn();
        if (newEvents.length > 0) {
          this.all.push(...newEvents);
          this.lastTimestamp = newEvents[newEvents.length - 1].timestamp;
          this.saveToCache();
        }
      }
    } finally {
      this.isRefreshing = false;
    }

    // 刷新完成后，如果没有订阅者则断开连接释放资源
    if (this.subscribers.size === 0 && this.isConnected) {
      this.disconnect();
    }
  }

  /**
   * 刷新事件数据，完成后如果有订阅者则自动开始轮询
   */
  async refreshWithPolling(fetchFn?: () => Promise<PoolEvent[]>): Promise<void> {
    await this.refresh(fetchFn);

    // 如果有订阅者，开始轮询
    if (this.subscribers.size > 0 && !this.isConnected) {
      this.connect();
      this.startPolling();
    }
  }

  /**
   * 开始轮询
   */
  startPolling(intervalMs = 5000, fetchFn?: () => Promise<PoolEvent[]>): void {
    if (this.refreshTimer) {
      return;
    }

    this.refreshTimer = setInterval(async () => {
      await this.refresh(fetchFn);

      // 检查是否应该停止轮询
      if (this.subscribers.size === 0 && this.all.length > 0) {
        this.stopPolling();
      }
    }, intervalMs);
  }

  /**
   * 停止轮询
   */
  stopPolling(): void {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
  }

  /**
   * 获取所有事件
   */
  getAll(): PoolEvent[] {
    return this.all;
  }

  /**
   * 获取最后时间戳
   */
  getLastTimestamp(): Date {
    return this.lastTimestamp;
  }
}
