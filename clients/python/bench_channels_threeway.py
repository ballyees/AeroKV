import asyncio
import time
from typing import List, Dict
from aerokv import AeroChannelLayer
from channels_redis.core import RedisChannelLayer


def calc_stats(latencies: List[float]):
    if not latencies:
        return {}
    s = sorted(latencies)
    n = len(s)
    return {
        "min": s[0],
        "p50": s[int(n * 0.50)],
        "p90": s[int(n * 0.90)],
        "p95": s[int(n * 0.95)],
        "p99": s[int(n * 0.99)],
        "max": s[-1],
        "avg": sum(s) / n,
    }


async def bench_p2p(layer, name: str, count: int = 2000):
    channel = await layer.new_channel("p2p.")
    latencies = []
    received = 0

    async def consumer():
        nonlocal received
        while received < count:
            msg = await layer.receive(channel)
            latencies.append((time.perf_counter() - msg["ts"]) * 1000.0)
            received += 1

    consumer_task = asyncio.create_task(consumer())
    await asyncio.sleep(0.05)

    start = time.perf_counter()
    for i in range(count):
        await layer.send(channel, {"i": i, "ts": time.perf_counter(), "data": "benchmark_payload_64bytes"})

    await consumer_task
    elapsed = time.perf_counter() - start
    stats = calc_stats(latencies)
    qps = count / elapsed

    return {
        "name": name,
        "count": count,
        "elapsed": elapsed,
        "qps": qps,
        "stats": stats,
    }


async def bench_group(layer, name: str, num_subs: int = 10, num_broadcasts: int = 200):
    clean_name = "".join(c for c in name.lower() if c.isalnum() or c in "_-.")
    group = f"grp_{clean_name}"
    channels = [await layer.new_channel(f"s{i}.") for i in range(num_subs)]

    for ch in channels:
        await layer.group_add(group, ch)

    delivered = 0
    latencies = []

    async def sub(ch):
        nonlocal delivered
        for _ in range(num_broadcasts):
            msg = await layer.receive(ch)
            latencies.append((time.perf_counter() - msg["ts"]) * 1000.0)
            delivered += 1

    tasks = [asyncio.create_task(sub(ch)) for ch in channels]
    await asyncio.sleep(0.05)

    start = time.perf_counter()
    for b in range(num_broadcasts):
        await layer.group_send(group, {"b": b, "ts": time.perf_counter(), "data": "broadcast_payload"})

    await asyncio.gather(*tasks)
    elapsed = time.perf_counter() - start
    stats = calc_stats(latencies)
    total_msgs = num_subs * num_broadcasts
    delivery_qps = total_msgs / elapsed

    return {
        "name": name,
        "broadcasts": num_broadcasts,
        "total_delivered": total_msgs,
        "elapsed": elapsed,
        "broadcast_qps": num_broadcasts / elapsed,
        "delivery_qps": delivery_qps,
        "stats": stats,
    }


async def main():
    print("================================================================")
    print("⚔️  Django Channels Three-Way Benchmark: AeroKV vs Redis vs Dragonfly")
    print("================================================================")

    aero_layer = AeroChannelLayer(host="127.0.0.1", port=16379, capacity=10_000)
    redis_layer = RedisChannelLayer(hosts=[("127.0.0.1", 6379)], capacity=10_000)
    dragonfly_layer = RedisChannelLayer(hosts=[("127.0.0.1", 6381)], capacity=10_000)

    # 1. P2P Benchmark
    print("\n▶ [1/2] Point-to-Point Benchmark (Send -> Receive, 2,000 msgs)")
    p2p_aero = await bench_p2p(aero_layer, "AeroKV (AeroChannelLayer)")
    p2p_redis = await bench_p2p(redis_layer, "Redis (RedisChannelLayer)")
    p2p_df = await bench_p2p(dragonfly_layer, "Dragonfly (RedisChannelLayer)")

    print(f"\n{'Target':<30} | {'Throughput':<14} | {'p50 Latency':<12} | {'p99 Latency':<12}")
    print("-" * 75)
    for res in [p2p_aero, p2p_redis, p2p_df]:
        print(f"{res['name']:<30} | {res['qps']:>10.1f} ops/s | {res['stats']['p50']:>9.3f} ms | {res['stats']['p99']:>9.3f} ms")

    # 2. Group Fan-Out Benchmark
    print("\n▶ [2/2] Group Broadcast Fan-Out (1 -> 10 subscribers, 200 broadcasts = 2,000 delivered)")
    grp_aero = await bench_group(aero_layer, "AeroKV (AeroChannelLayer)")
    grp_redis = await bench_group(redis_layer, "Redis (RedisChannelLayer)")
    grp_df = await bench_group(dragonfly_layer, "Dragonfly (RedisChannelLayer)")

    print(f"\n{'Target':<30} | {'Delivery QPS':<14} | {'p50 Latency':<12} | {'p99 Latency':<12}")
    print("-" * 75)
    for res in [grp_aero, grp_redis, grp_df]:
        print(f"{res['name']:<30} | {res['delivery_qps']:>10.1f} ops/s | {res['stats']['p50']:>9.3f} ms | {res['stats']['p99']:>9.3f} ms")

if __name__ == "__main__":
    asyncio.run(main())
